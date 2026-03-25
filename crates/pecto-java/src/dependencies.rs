use crate::context::AnalysisContext;
use crate::extractors::common::*;
use pecto_core::model::*;

/// Resolve dependencies between capabilities by analyzing field declarations and method bodies.
pub fn resolve_dependencies(spec: &mut ProjectSpec, ctx: &AnalysisContext) {
    let capability_names: Vec<String> = spec.capabilities.iter().map(|c| c.name.clone()).collect();

    for file in &ctx.files {
        let root = file.tree.root_node();
        let source = file.source.as_bytes();

        for i in 0..root.named_child_count() {
            let node = root.named_child(i).unwrap();
            if node.kind() != "class_declaration" {
                continue;
            }

            let class_name = get_class_name(&node, source);
            let from_capability = find_capability_for_class(&class_name, &spec.capabilities);
            let Some(from_cap) = from_capability else {
                continue;
            };

            // Extract field declarations to build a type map: field_name → type_name
            let field_types = extract_field_types(&node, source);

            // Scan method bodies for calls to injected dependencies
            let Some(body) = node.child_by_field_name("body") else {
                continue;
            };

            for j in 0..body.named_child_count() {
                let member = body.named_child(j).unwrap();
                if member.kind() != "method_declaration" {
                    continue;
                }

                let Some(method_body) = member.child_by_field_name("body") else {
                    continue;
                };

                let method_name = member
                    .child_by_field_name("name")
                    .map(|n| node_text(&n, source))
                    .unwrap_or_default();

                // Find method invocations on fields
                let calls = extract_method_calls(&method_body, source);
                for (field_name, called_method) in &calls {
                    if let Some(field_type) = field_types.get(field_name.as_str())
                        && let Some(to_cap) =
                            resolve_type_to_capability(field_type, &capability_names)
                        && to_cap != from_cap
                    {
                        let reference = format!(
                            "{}#{} → {}.{}",
                            class_name, method_name, field_name, called_method
                        );
                        add_dependency_edge(
                            &mut spec.dependencies,
                            &from_cap,
                            &to_cap,
                            infer_dependency_kind(&to_cap),
                            reference,
                        );
                    }
                }
            }
        }
    }
}

/// Extract field declarations: field_name → type_name.
fn extract_field_types(
    class_node: &tree_sitter::Node,
    source: &[u8],
) -> std::collections::HashMap<String, String> {
    let mut fields = std::collections::HashMap::new();
    let Some(body) = class_node.child_by_field_name("body") else {
        return fields;
    };

    for i in 0..body.named_child_count() {
        let member = body.named_child(i).unwrap();
        if member.kind() != "field_declaration" {
            continue;
        }

        let field_type = member
            .child_by_field_name("type")
            .map(|t| node_text(&t, source))
            .unwrap_or_default();

        // Extract variable name from declarator
        for j in 0..member.named_child_count() {
            let child = member.named_child(j).unwrap();
            if child.kind() == "variable_declarator"
                && let Some(name) = child.child_by_field_name("name")
            {
                fields.insert(node_text(&name, source), field_type.clone());
            }
        }
    }

    fields
}

/// Extract method calls from a method body: Vec<(field_name, method_name)>.
fn extract_method_calls(node: &tree_sitter::Node, source: &[u8]) -> Vec<(String, String)> {
    let mut calls = Vec::new();
    collect_method_calls(node, source, &mut calls);
    calls
}

fn collect_method_calls(
    node: &tree_sitter::Node,
    source: &[u8],
    calls: &mut Vec<(String, String)>,
) {
    if node.kind() == "method_invocation" {
        let text = node_text(node, source);
        // Pattern: fieldName.methodName(...)
        if let Some(dot_pos) = text.find('.') {
            let field = text[..dot_pos].trim();
            let rest = &text[dot_pos + 1..];
            if let Some(paren_pos) = rest.find('(') {
                let method = rest[..paren_pos].trim();
                if !field.is_empty()
                    && !method.is_empty()
                    && field.chars().next().is_some_and(|c| c.is_lowercase())
                {
                    calls.push((field.to_string(), method.to_string()));
                    return; // Don't recurse into this node's children
                }
            }
        }
    }

    for i in 0..node.child_count() {
        let child = node.child(i).unwrap();
        collect_method_calls(&child, source, calls);
    }
}

/// Find which capability a class belongs to.
fn find_capability_for_class(class_name: &str, capabilities: &[Capability]) -> Option<String> {
    // Check source file paths and naming conventions
    for cap in capabilities {
        // Direct name match via source path
        if cap.source.contains(class_name) {
            return Some(cap.name.clone());
        }
    }

    // Try kebab-case conversion
    let kebab = to_kebab_case(class_name);
    for cap in capabilities {
        if kebab.starts_with(&cap.name) || cap.name.starts_with(&kebab) {
            return Some(cap.name.clone());
        }
    }

    None
}

/// Resolve a Java type name to a capability name.
fn resolve_type_to_capability(type_name: &str, capability_names: &[String]) -> Option<String> {
    let kebab_full = to_kebab_case(type_name);

    // Direct match: OrderRepository → order-repository
    if capability_names.contains(&kebab_full) {
        return Some(kebab_full);
    }

    // Strip common suffixes and try with different suffixes
    let base = type_name
        .replace("Repository", "")
        .replace("Service", "")
        .replace("Controller", "")
        .replace("Impl", "");
    let kebab_base = to_kebab_case(&base);

    let candidates = [
        format!("{}-service", kebab_base),
        format!("{}-repository", kebab_base),
        format!("{}-controller", kebab_base),
        format!("{}-entity", kebab_base),
        kebab_base.clone(),
    ];

    for candidate in &candidates {
        if capability_names.contains(candidate) {
            return Some(candidate.clone());
        }
    }

    // Partial match: if any capability name starts with or contains the base
    for cap in capability_names {
        if cap.starts_with(&kebab_base) || kebab_base.starts_with(cap.as_str()) {
            return Some(cap.clone());
        }
    }

    None
}

fn infer_dependency_kind(target_capability: &str) -> DependencyKind {
    if target_capability.contains("repository") {
        DependencyKind::Queries
    } else if target_capability.contains("listener") || target_capability.contains("handler") {
        DependencyKind::Listens
    } else {
        DependencyKind::Calls
    }
}

fn add_dependency_edge(
    deps: &mut Vec<DependencyEdge>,
    from: &str,
    to: &str,
    kind: DependencyKind,
    reference: String,
) {
    // Check if edge already exists
    if let Some(existing) = deps.iter_mut().find(|d| d.from == from && d.to == to) {
        if !existing.references.contains(&reference) {
            existing.references.push(reference);
        }
    } else {
        deps.push(DependencyEdge {
            from: from.to_string(),
            to: to.to_string(),
            kind,
            references: vec![reference],
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ParsedFile;

    fn parse_file(source: &str, path: &str) -> ParsedFile {
        ParsedFile::parse(source.to_string(), path.to_string()).unwrap()
    }

    #[test]
    fn test_controller_to_service_dependency() {
        let controller_source = r#"
@RestController
@RequestMapping("/api/users")
public class UserController {
    private final UserService userService;

    @GetMapping("/{id}")
    public User getById(@PathVariable Long id) {
        return userService.findById(id);
    }

    @PostMapping
    public User create(@RequestBody CreateUserRequest req) {
        return userService.create(req);
    }
}
"#;

        let service_source = r#"
@Service
public class UserService {
    public User findById(Long id) { return null; }
    public User create(CreateUserRequest req) { return null; }
}
"#;

        let controller_file = parse_file(controller_source, "src/UserController.java");
        let service_file = parse_file(service_source, "src/UserService.java");

        let ctx = AnalysisContext::new(vec![controller_file, service_file]);

        let mut spec = ProjectSpec::new("test");
        // Simulate extracted capabilities
        spec.capabilities
            .push(Capability::new("user", "src/UserController.java"));
        spec.capabilities[0].endpoints.push(Endpoint {
            method: HttpMethod::Get,
            path: "/api/users/{id}".to_string(),
            input: None,
            validation: Vec::new(),
            behaviors: Vec::new(),
            security: None,
        });
        spec.capabilities
            .push(Capability::new("user-service", "src/UserService.java"));

        resolve_dependencies(&mut spec, &ctx);

        assert!(
            !spec.dependencies.is_empty(),
            "Should find controller → service dependency"
        );

        let dep = spec
            .dependencies
            .iter()
            .find(|d| d.from == "user" && d.to == "user-service");
        assert!(dep.is_some(), "Should have user → user-service edge");
        assert!(matches!(dep.unwrap().kind, DependencyKind::Calls));
    }

    #[test]
    fn test_service_to_repository_dependency() {
        let service_source = r#"
@Service
public class OrderService {
    private final OrderRepository orderRepository;

    public Order create(CreateOrderRequest req) {
        Order order = new Order(req);
        return orderRepository.save(order);
    }
}
"#;

        let service_file = parse_file(service_source, "src/OrderService.java");
        let ctx = AnalysisContext::new(vec![service_file]);

        let mut spec = ProjectSpec::new("test");
        spec.capabilities
            .push(Capability::new("order-service", "src/OrderService.java"));
        spec.capabilities.push(Capability::new(
            "order-repository",
            "src/OrderRepository.java",
        ));
        spec.capabilities[1].operations.push(Operation {
            name: "save".to_string(),
            source_method: "OrderRepository#save".to_string(),
            input: None,
            behaviors: Vec::new(),
            transaction: None,
        });

        resolve_dependencies(&mut spec, &ctx);

        let dep = spec
            .dependencies
            .iter()
            .find(|d| d.from == "order-service" && d.to == "order-repository");
        assert!(
            dep.is_some(),
            "Should have order-service → order-repository edge. Found: {:?}",
            spec.dependencies
        );
        assert!(matches!(dep.unwrap().kind, DependencyKind::Queries));
    }
}
