use super::common::*;
use crate::context::ParsedFile;
use pecto_core::model::*;
use tree_sitter::Node;

/// Extract a Capability from a parsed file if it contains a Spring @Service class.
pub fn extract(file: &ParsedFile) -> Option<Capability> {
    let root = file.tree.root_node();
    let source = file.source.as_bytes();

    for i in 0..root.named_child_count() {
        let node = root.named_child(i).unwrap();
        if node.kind() != "class_declaration" {
            continue;
        }

        let annotations = collect_annotations(&node, source);
        let is_service = annotations
            .iter()
            .any(|a| a.name == "Service" || a.name == "Component");

        if !is_service {
            continue;
        }

        let class_name = get_class_name(&node, source);
        let capability_name = format!(
            "{}-service",
            to_kebab_case(&class_name.replace("Service", "").replace("Impl", ""))
        );

        let mut capability = Capability::new(capability_name, file.path.clone());

        // Check for class-level @Transactional
        let class_transactional = extract_transactional(&annotations);

        if let Some(body) = node.child_by_field_name("body") {
            extract_service_methods(
                &body,
                source,
                &class_name,
                class_transactional.as_deref(),
                &mut capability,
            );
        }

        return Some(capability);
    }

    None
}

fn extract_service_methods(
    body: &Node,
    source: &[u8],
    class_name: &str,
    class_transactional: Option<&str>,
    capability: &mut Capability,
) {
    for i in 0..body.named_child_count() {
        let member = body.named_child(i).unwrap();
        if member.kind() != "method_declaration" {
            continue;
        }

        // Only extract public methods
        if !is_public(&member, source) {
            continue;
        }

        let method_name = member
            .child_by_field_name("name")
            .map(|n| node_text(&n, source))
            .unwrap_or_default();

        if method_name.is_empty() {
            continue;
        }

        let annotations = collect_annotations(&member, source);

        // Method-level @Transactional overrides class-level
        let transaction =
            extract_transactional(&annotations).or_else(|| class_transactional.map(String::from));

        // Scan method body for side effects
        let side_effects = member
            .child_by_field_name("body")
            .map(|body| extract_side_effects(&body, source))
            .unwrap_or_default();

        let operation = Operation {
            name: method_name.clone(),
            source_method: format!("{}#{}", class_name, method_name),
            input: extract_operation_input(&member, source),
            behaviors: vec![Behavior {
                name: "success".to_string(),
                condition: None,
                returns: ResponseSpec {
                    status: 200,
                    body: member
                        .child_by_field_name("type")
                        .map(|t| node_text(&t, source))
                        .filter(|t| t != "void")
                        .map(|t| TypeRef {
                            name: clean_generic_type(&t),
                            fields: std::collections::BTreeMap::new(),
                        }),
                },
                side_effects,
            }],
            transaction,
        };

        capability.operations.push(operation);
    }
}

/// Extract @Transactional annotation details.
fn extract_transactional(annotations: &[AnnotationInfo]) -> Option<String> {
    let ann = annotations.iter().find(|a| a.name == "Transactional")?;

    let read_only = ann
        .arguments
        .get("readOnly")
        .map(|v| v == "true")
        .unwrap_or(false);

    let propagation = ann.arguments.get("propagation").cloned();

    if read_only {
        Some("read-only".to_string())
    } else if let Some(prop) = propagation {
        // e.g., Propagation.REQUIRES_NEW -> requires-new
        let clean = prop
            .replace("Propagation.", "")
            .to_lowercase()
            .replace('_', "-");
        Some(clean)
    } else {
        Some("required".to_string())
    }
}

/// Check if a method has public access.
fn is_public(node: &Node, source: &[u8]) -> bool {
    for i in 0..node.named_child_count() {
        let child = node.named_child(i).unwrap();
        if child.kind() == "modifiers" {
            for j in 0..child.child_count() {
                let m = child.child(j).unwrap();
                if node_text(&m, source) == "public" {
                    return true;
                }
            }
        }
    }
    false
}

/// Extract side effects from a method body by scanning for known patterns.
fn extract_side_effects(body: &Node, source: &[u8]) -> Vec<SideEffect> {
    let mut effects = Vec::new();
    collect_side_effects(body, source, &mut effects);
    effects
}

fn collect_side_effects(node: &Node, source: &[u8], effects: &mut Vec<SideEffect>) {
    if node.kind() == "method_invocation" {
        let text = node_text(node, source);

        // Repository calls: xxx.save(...), xxx.delete(...), xxx.saveAll(...)
        if text.contains(".save(") || text.contains(".saveAll(") {
            let target = extract_call_target(&text);
            if !effects
                .iter()
                .any(|e| matches!(e, SideEffect::DbInsert { table } if table == &target))
            {
                effects.push(SideEffect::DbInsert { table: target });
            }
        } else if text.contains(".delete(")
            || text.contains(".deleteById(")
            || text.contains(".deleteAll(")
        {
            let target = extract_call_target(&text);
            if !effects.iter().any(|e| matches!(e, SideEffect::DbUpdate { description } if description.contains(&target))) {
                effects.push(SideEffect::DbUpdate {
                    description: format!("delete via {}", target),
                });
            }
        }

        // Event publishing: publishEvent(new XxxEvent(...))
        if text.contains("publishEvent(")
            && let Some(event_name) = extract_event_name(&text)
            && !effects
                .iter()
                .any(|e| matches!(e, SideEffect::Event { name } if name == &event_name))
        {
            effects.push(SideEffect::Event { name: event_name });
        }

        // Service calls: xxxService.methodName(...)
        if text.contains("Service.") || text.contains("service.") {
            let target = extract_service_call_target(&text);
            if !target.is_empty()
                && !effects
                    .iter()
                    .any(|e| matches!(e, SideEffect::ServiceCall { target: t } if t == &target))
            {
                effects.push(SideEffect::ServiceCall { target });
            }
        }
    }

    for i in 0..node.child_count() {
        let child = node.child(i).unwrap();
        collect_side_effects(&child, source, effects);
    }
}

/// Extract the object name before a method call: "userRepository.save(x)" -> "userRepository"
fn extract_call_target(text: &str) -> String {
    text.split('.')
        .next()
        .unwrap_or("unknown")
        .trim()
        .to_string()
}

/// Extract event name from publishEvent(new XxxEvent(...))
fn extract_event_name(text: &str) -> Option<String> {
    let after_new = text.split("new ").nth(1)?;
    let class_name = after_new.split('(').next()?.trim();
    if class_name.is_empty() {
        return None;
    }
    Some(class_name.to_string())
}

/// Extract service call target: "notificationService.sendEmail(...)" -> "notificationService.sendEmail"
fn extract_service_call_target(text: &str) -> String {
    // Take everything before the first '('
    let before_paren = text.split('(').next().unwrap_or("");
    before_paren.trim().to_string()
}

/// Extract input type from the first parameter of a service method.
fn extract_operation_input(method_node: &Node, source: &[u8]) -> Option<TypeRef> {
    let params = method_node.child_by_field_name("parameters")?;
    for i in 0..params.named_child_count() {
        let param = params.named_child(i).unwrap();
        if param.kind() == "formal_parameter" {
            let type_name = param
                .child_by_field_name("type")
                .map(|t| node_text(&t, source))?;
            return Some(TypeRef {
                name: clean_generic_type(&type_name),
                fields: std::collections::BTreeMap::new(),
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ParsedFile;

    fn parse_file(source: &str, path: &str) -> ParsedFile {
        ParsedFile::parse(source.to_string(), path.to_string()).unwrap()
    }

    #[test]
    fn test_basic_service() {
        let source = r#"
@Service
public class UserService {

    public User findById(Long id) {
        return userRepository.findById(id).orElse(null);
    }

    public User create(CreateUserRequest request) {
        User user = new User(request);
        return userRepository.save(user);
    }

    public void delete(Long id) {
        userRepository.deleteById(id);
    }

    private void internalMethod() {
        // should not be extracted
    }
}
"#;

        let file = parse_file(source, "src/UserService.java");
        let capability = extract(&file).unwrap();

        assert_eq!(capability.name, "user-service");
        // Only public methods: findById, create, delete
        assert_eq!(capability.operations.len(), 3);

        let create = capability
            .operations
            .iter()
            .find(|o| o.name == "create")
            .unwrap();
        assert!(create.input.is_some());
        assert_eq!(create.input.as_ref().unwrap().name, "CreateUserRequest");
        // Should detect save side effect
        assert!(
            create.behaviors[0]
                .side_effects
                .iter()
                .any(|e| matches!(e, SideEffect::DbInsert { .. }))
        );

        let delete = capability
            .operations
            .iter()
            .find(|o| o.name == "delete")
            .unwrap();
        assert!(
            delete.behaviors[0]
                .side_effects
                .iter()
                .any(|e| matches!(e, SideEffect::DbUpdate { .. }))
        );
    }

    #[test]
    fn test_transactional() {
        let source = r#"
@Service
@Transactional
public class OrderService {

    @Transactional(readOnly = true)
    public Order findById(Long id) {
        return orderRepository.findById(id).orElse(null);
    }

    public Order createOrder(CreateOrderRequest request) {
        Order order = new Order(request);
        orderRepository.save(order);
        eventPublisher.publishEvent(new OrderCreatedEvent(order));
        return order;
    }

    @Transactional(propagation = Propagation.REQUIRES_NEW)
    public void processPayment(Long orderId) {
        paymentService.charge(orderId);
    }
}
"#;

        let file = parse_file(source, "src/OrderService.java");
        let capability = extract(&file).unwrap();

        assert_eq!(capability.name, "order-service");

        // findById: read-only overrides class-level
        let find = capability
            .operations
            .iter()
            .find(|o| o.name == "findById")
            .unwrap();
        assert_eq!(find.transaction.as_deref(), Some("read-only"));

        // createOrder: inherits class-level "required"
        let create = capability
            .operations
            .iter()
            .find(|o| o.name == "createOrder")
            .unwrap();
        assert_eq!(create.transaction.as_deref(), Some("required"));
        // Side effects: db insert + event
        assert!(
            create.behaviors[0]
                .side_effects
                .iter()
                .any(|e| matches!(e, SideEffect::DbInsert { .. }))
        );
        assert!(
            create.behaviors[0]
                .side_effects
                .iter()
                .any(|e| matches!(e, SideEffect::Event { name } if name == "OrderCreatedEvent"))
        );

        // processPayment: REQUIRES_NEW
        let payment = capability
            .operations
            .iter()
            .find(|o| o.name == "processPayment")
            .unwrap();
        assert_eq!(payment.transaction.as_deref(), Some("requires-new"));
        // Side effect: service call
        assert!(
            payment.behaviors[0]
                .side_effects
                .iter()
                .any(|e| matches!(e, SideEffect::ServiceCall { .. }))
        );
    }

    #[test]
    fn test_non_service_class() {
        let source = r#"
public class UserController {
    public void handleRequest() {}
}
"#;

        let file = parse_file(source, "src/UserController.java");
        assert!(extract(&file).is_none());
    }
}
