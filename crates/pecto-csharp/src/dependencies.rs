use crate::context::AnalysisContext;
use crate::extractors::common::*;
use pecto_core::model::*;

/// Resolve dependencies between C# capabilities.
/// Uses constructor injection patterns and field/method analysis.
pub fn resolve_dependencies(spec: &mut ProjectSpec, ctx: &AnalysisContext) {
    let capability_names: Vec<String> = spec.capabilities.iter().map(|c| c.name.clone()).collect();

    for file in &ctx.files {
        let root = file.tree.root_node();
        let source = file.source.as_bytes();

        for_each_class(&root, source, &mut |node, src| {
            let class_name = get_class_name(node, src);
            let Some(from_cap) = find_capability_for_class(&class_name, &spec.capabilities) else {
                return;
            };

            // C# uses constructor injection — extract constructor params as dependencies
            let injected_types = extract_constructor_injections(node, src);
            // Also check readonly fields (backing fields for injected services)
            let field_types = extract_field_types(node, src);

            let Some(body) = node.child_by_field_name("body") else {
                return;
            };

            // For constructor-injected types, create direct dependency edges
            for (param_name, param_type) in &injected_types {
                if let Some(to_cap) = resolve_type_to_capability(param_type, &capability_names)
                    && to_cap != from_cap
                {
                    add_dependency_edge(
                        &mut spec.dependencies,
                        &from_cap,
                        &to_cap,
                        infer_dependency_kind(&to_cap),
                        format!(
                            "{} constructor injects {}: {}",
                            class_name, param_name, param_type
                        ),
                    );
                }
            }

            // Scan method bodies for field usage
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
                    .map(|n| node_text(&n, src))
                    .unwrap_or_default();

                let calls = extract_method_calls(&method_body, src);
                for (field_name, called_method) in &calls {
                    if let Some(field_type) = field_types.get(field_name.as_str())
                        && let Some(to_cap) =
                            resolve_type_to_capability(field_type, &capability_names)
                        && to_cap != from_cap
                    {
                        add_dependency_edge(
                            &mut spec.dependencies,
                            &from_cap,
                            &to_cap,
                            infer_dependency_kind(&to_cap),
                            format!(
                                "{}#{} → {}.{}",
                                class_name, method_name, field_name, called_method
                            ),
                        );
                    }
                }
            }
        });
    }
}

/// Extract constructor parameter types (C# DI pattern).
fn extract_constructor_injections(
    class_node: &tree_sitter::Node,
    source: &[u8],
) -> Vec<(String, String)> {
    let mut injections = Vec::new();
    let Some(body) = class_node.child_by_field_name("body") else {
        return injections;
    };

    for i in 0..body.named_child_count() {
        let member = body.named_child(i).unwrap();
        if member.kind() != "constructor_declaration" {
            continue;
        }

        let Some(params) = member.child_by_field_name("parameters") else {
            continue;
        };

        for j in 0..params.named_child_count() {
            let param = params.named_child(j).unwrap();
            if param.kind() != "parameter" {
                continue;
            }

            let param_type = param
                .child_by_field_name("type")
                .map(|t| node_text(&t, source))
                .unwrap_or_default();
            let param_name = param
                .child_by_field_name("name")
                .map(|n| node_text(&n, source))
                .unwrap_or_default();

            // Strip I prefix for interface types: IOrderService → OrderService
            let resolved_type = if param_type.starts_with('I') && param_type.len() > 1 {
                param_type[1..].to_string()
            } else {
                param_type.clone()
            };

            injections.push((param_name, resolved_type));
        }
    }

    injections
}

/// Extract field types from class body (readonly fields backing injected services).
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

        // Strip I prefix for interfaces
        let resolved_type = if field_type.starts_with('I') && field_type.len() > 1 {
            field_type[1..].to_string()
        } else {
            field_type.clone()
        };

        for j in 0..member.named_child_count() {
            let child = member.named_child(j).unwrap();
            if child.kind() == "variable_declaration" {
                for k in 0..child.named_child_count() {
                    let declarator = child.named_child(k).unwrap();
                    if declarator.kind() == "variable_declarator"
                        && let Some(name) = declarator.child_by_field_name("name")
                    {
                        let field_name = node_text(&name, source);
                        fields.insert(field_name.clone(), resolved_type.clone());
                        if let Some(stripped) = field_name.strip_prefix('_') {
                            fields.insert(stripped.to_string(), resolved_type.clone());
                        }
                    }
                }
            }
        }
    }

    fields
}

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
    if node.kind() == "invocation_expression" {
        let text = node_text(node, source);
        if let Some(dot_pos) = text.find('.') {
            let obj = text[..dot_pos].trim().trim_start_matches("await ");
            let rest = &text[dot_pos + 1..];
            if let Some(paren_pos) = rest.find('(') {
                let method = rest[..paren_pos].trim();
                if !obj.is_empty() && !method.is_empty() {
                    calls.push((obj.to_string(), method.to_string()));
                    return;
                }
            }
        }
    }

    for i in 0..node.child_count() {
        let child = node.child(i).unwrap();
        collect_method_calls(&child, source, calls);
    }
}

fn find_capability_for_class(class_name: &str, capabilities: &[Capability]) -> Option<String> {
    for cap in capabilities {
        if cap.source.contains(class_name) {
            return Some(cap.name.clone());
        }
    }

    let kebab = to_kebab_case(class_name);
    for cap in capabilities {
        if kebab.starts_with(&cap.name) || cap.name.starts_with(&kebab) {
            return Some(cap.name.clone());
        }
    }

    None
}

fn resolve_type_to_capability(type_name: &str, capability_names: &[String]) -> Option<String> {
    let kebab_full = to_kebab_case(type_name);

    if capability_names.contains(&kebab_full) {
        return Some(kebab_full);
    }

    let base = type_name
        .replace("Repository", "")
        .replace("Service", "")
        .replace("Controller", "")
        .replace("Context", "");
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

    for cap in capability_names {
        if cap.starts_with(&kebab_base) || kebab_base.starts_with(cap.as_str()) {
            return Some(cap.clone());
        }
    }

    None
}

fn infer_dependency_kind(target_capability: &str) -> DependencyKind {
    if target_capability.contains("repository") || target_capability.contains("context") {
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
    fn test_constructor_injection_dependency() {
        let controller_source = r#"
namespace MyApp.Controllers;

[ApiController]
[Route("api/[controller]")]
public class OrdersController : ControllerBase
{
    private readonly IOrderService _orderService;

    public OrdersController(IOrderService orderService)
    {
        _orderService = orderService;
    }

    [HttpGet("{id}")]
    public async Task<ActionResult<Order>> GetById(int id)
    {
        return Ok(await _orderService.FindById(id));
    }
}
"#;

        let file = parse_file(controller_source, "Controllers/OrdersController.cs");
        let ctx = AnalysisContext::new(vec![file]);

        let mut spec = ProjectSpec::new("test");
        spec.capabilities
            .push(Capability::new("orders", "Controllers/OrdersController.cs"));
        spec.capabilities[0].endpoints.push(Endpoint {
            method: HttpMethod::Get,
            path: "api/orders/{id}".to_string(),
            input: None,
            validation: Vec::new(),
            behaviors: Vec::new(),
            security: None,
        });
        spec.capabilities
            .push(Capability::new("order-service", "Services/OrderService.cs"));

        resolve_dependencies(&mut spec, &ctx);

        let dep = spec
            .dependencies
            .iter()
            .find(|d| d.from == "orders" && d.to == "order-service");
        assert!(
            dep.is_some(),
            "Should find orders → order-service dependency. Found: {:?}",
            spec.dependencies
        );
    }

    #[test]
    fn test_service_to_service_dependency() {
        let service_source = r#"
namespace MyApp.Services;

public class PaymentService : IPaymentService
{
    private readonly IOrderService _orderService;
    private readonly INotificationService _notificationService;

    public PaymentService(IOrderService orderService, INotificationService notificationService)
    {
        _orderService = orderService;
        _notificationService = notificationService;
    }

    public async Task ProcessPayment(int orderId)
    {
        await _orderService.MarkAsPaid(orderId);
        await _notificationService.SendConfirmation(orderId);
    }
}
"#;

        let file = parse_file(service_source, "Services/PaymentService.cs");
        let ctx = AnalysisContext::new(vec![file]);

        let mut spec = ProjectSpec::new("test");
        spec.capabilities.push(Capability::new(
            "payment-service",
            "Services/PaymentService.cs",
        ));
        spec.capabilities
            .push(Capability::new("order-service", "Services/OrderService.cs"));
        spec.capabilities.push(Capability::new(
            "notification-service",
            "Services/NotificationService.cs",
        ));

        resolve_dependencies(&mut spec, &ctx);

        assert!(
            spec.dependencies
                .iter()
                .any(|d| d.from == "payment-service" && d.to == "order-service"),
            "Should find payment → order dependency. Found: {:?}",
            spec.dependencies
        );
        assert!(
            spec.dependencies
                .iter()
                .any(|d| d.from == "payment-service" && d.to == "notification-service"),
            "Should find payment → notification dependency"
        );
    }
}
