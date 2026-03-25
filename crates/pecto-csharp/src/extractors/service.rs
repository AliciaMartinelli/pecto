use super::common::*;
use crate::context::ParsedFile;
use pecto_core::model::*;
use tree_sitter::Node;

/// Extract a Capability from a parsed file if it contains a C# service class.
/// Detection heuristic: class name ends in "Service" and is public,
/// or class implements an I*Service interface.
pub fn extract(file: &ParsedFile) -> Option<Capability> {
    let root = file.tree.root_node();
    let source = file.source.as_bytes();

    let mut result = None;
    for_each_class(&root, source, &mut |node, src| {
        if result.is_some() {
            return;
        }

        let class_name = get_class_name(node, src);
        if !is_service_class(node, src, &class_name) {
            return;
        }

        let capability_name = format!(
            "{}-service",
            to_kebab_case(&class_name.replace("Service", "").replace("Impl", ""))
        );
        let mut capability = Capability::new(capability_name, file.path.clone());

        if let Some(body) = node.child_by_field_name("body") {
            extract_service_methods(&body, src, &class_name, &mut capability);
        }

        result = Some(capability);
    });

    result
}

fn is_service_class(node: &Node, source: &[u8], class_name: &str) -> bool {
    // Must be public
    if !is_public(node, source) {
        return false;
    }

    // Heuristic 1: name ends in "Service"
    if class_name.ends_with("Service") {
        return true;
    }

    // Heuristic 2: implements I*Service interface
    for i in 0..node.named_child_count() {
        let child = node.named_child(i).unwrap();
        if child.kind() == "base_list" {
            let text = node_text(&child, source);
            // Check for IXxxService pattern
            if text.contains("Service") && text.contains('I') {
                return true;
            }
        }
    }

    false
}

fn is_public(node: &Node, source: &[u8]) -> bool {
    for i in 0..node.child_count() {
        let child = node.child(i).unwrap();
        if node_text(&child, source) == "public" {
            return true;
        }
    }
    false
}

fn extract_service_methods(
    body: &Node,
    source: &[u8],
    class_name: &str,
    capability: &mut Capability,
) {
    for i in 0..body.named_child_count() {
        let member = body.named_child(i).unwrap();
        if member.kind() != "method_declaration" {
            continue;
        }

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

        let return_type = member
            .child_by_field_name("returns")
            .or_else(|| member.child_by_field_name("type"))
            .map(|t| node_text(&t, source));

        let transaction = member
            .child_by_field_name("body")
            .map(|b| detect_transaction(&b, source))
            .unwrap_or(None);

        let side_effects = member
            .child_by_field_name("body")
            .map(|b| extract_side_effects(&b, source))
            .unwrap_or_default();

        let input = extract_first_param(&member, source);

        let operation = Operation {
            name: method_name.clone(),
            source_method: format!("{}#{}", class_name, method_name),
            input,
            behaviors: vec![Behavior {
                name: "success".to_string(),
                condition: None,
                returns: ResponseSpec {
                    status: 200,
                    body: return_type
                        .filter(|t| t != "void" && t != "Task")
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

fn detect_transaction(body: &Node, source: &[u8]) -> Option<String> {
    let text = node_text(body, source);

    if text.contains("BeginTransaction") || text.contains("using var transaction") {
        Some("explicit".to_string())
    } else if text.contains("SaveChangesAsync")
        || text.contains("SaveChanges")
        || (text.contains("_unitOfWork") && text.contains("Commit"))
    {
        Some("unit-of-work".to_string())
    } else {
        None
    }
}

fn extract_side_effects(body: &Node, source: &[u8]) -> Vec<SideEffect> {
    let mut effects = Vec::new();
    let text = node_text(body, source);

    // EF Core Add/AddAsync patterns
    if text.contains(".Add(") || text.contains(".AddAsync(") || text.contains(".AddRange(") {
        effects.push(SideEffect::DbInsert {
            table: "via EF Core".to_string(),
        });
    }

    // EF Core Remove patterns
    if text.contains(".Remove(") || text.contains(".RemoveRange(") {
        effects.push(SideEffect::DbUpdate {
            description: "delete via EF Core".to_string(),
        });
    }

    // EF Core Update
    if text.contains(".Update(") || text.contains(".UpdateRange(") {
        effects.push(SideEffect::DbUpdate {
            description: "update via EF Core".to_string(),
        });
    }

    // MediatR event publishing
    if (text.contains(".Publish(") || text.contains(".Send("))
        && let Some(event_name) = extract_event_name(&text)
    {
        effects.push(SideEffect::Event { name: event_name });
    }

    // Service calls: _xxxService.Method(...)
    for line in text.lines() {
        let trimmed = line.trim();
        if (trimmed.contains("Service.") || trimmed.contains("service.")) && trimmed.contains('(') {
            let target = trimmed
                .split('(')
                .next()
                .unwrap_or("")
                .trim()
                .trim_start_matches("await ")
                .trim();
            if !target.is_empty()
                && !effects
                    .iter()
                    .any(|e| matches!(e, SideEffect::ServiceCall { target: t } if t == target))
            {
                effects.push(SideEffect::ServiceCall {
                    target: target.to_string(),
                });
            }
        }
    }

    effects
}

fn extract_event_name(text: &str) -> Option<String> {
    let after_new = text.split("new ").nth(1)?;
    let class_name = after_new.split(['(', '{', ' ']).next()?.trim();
    if class_name.is_empty() {
        return None;
    }
    Some(class_name.to_string())
}

fn extract_first_param(method_node: &Node, source: &[u8]) -> Option<TypeRef> {
    let params = method_node.child_by_field_name("parameters")?;
    for i in 0..params.named_child_count() {
        let param = params.named_child(i).unwrap();
        if param.kind() == "parameter" {
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
namespace MyApp.Services;

public class OrderService
{
    private readonly AppDbContext _context;

    public async Task<Order> CreateOrder(CreateOrderRequest request)
    {
        var order = new Order(request);
        _context.Add(order);
        await _context.SaveChangesAsync();
        return order;
    }

    public async Task DeleteOrder(int id)
    {
        var order = await _context.Orders.FindAsync(id);
        _context.Remove(order);
        await _context.SaveChangesAsync();
    }

    private void InternalHelper() { }
}
"#;

        let file = parse_file(source, "Services/OrderService.cs");
        let capability = extract(&file).unwrap();

        assert_eq!(capability.name, "order-service");
        // Only public methods: CreateOrder, DeleteOrder
        assert_eq!(capability.operations.len(), 2);

        let create = capability
            .operations
            .iter()
            .find(|o| o.name == "CreateOrder")
            .unwrap();
        assert!(create.input.is_some());
        assert_eq!(create.transaction.as_deref(), Some("unit-of-work"));
        assert!(
            create.behaviors[0]
                .side_effects
                .iter()
                .any(|e| matches!(e, SideEffect::DbInsert { .. }))
        );

        let delete = capability
            .operations
            .iter()
            .find(|o| o.name == "DeleteOrder")
            .unwrap();
        assert!(
            delete.behaviors[0]
                .side_effects
                .iter()
                .any(|e| matches!(e, SideEffect::DbUpdate { .. }))
        );
    }

    #[test]
    fn test_service_with_mediator() {
        let source = r#"
namespace MyApp.Services;

public class PaymentService : IPaymentService
{
    private readonly IMediator _mediator;
    private readonly IOrderService _orderService;

    public async Task ProcessPayment(PaymentRequest request)
    {
        using var transaction = await _context.Database.BeginTransactionAsync();
        await _mediator.Publish(new PaymentProcessedEvent(request.OrderId));
        await _orderService.MarkAsPaid(request.OrderId);
    }
}
"#;

        let file = parse_file(source, "Services/PaymentService.cs");
        let capability = extract(&file).unwrap();

        assert_eq!(capability.name, "payment-service");
        let process = &capability.operations[0];
        assert_eq!(process.transaction.as_deref(), Some("explicit"));
        assert!(
            process.behaviors[0]
                .side_effects
                .iter()
                .any(|e| matches!(e, SideEffect::Event { .. }))
        );
        assert!(
            process.behaviors[0]
                .side_effects
                .iter()
                .any(|e| matches!(e, SideEffect::ServiceCall { .. }))
        );
    }

    #[test]
    fn test_non_service_class() {
        let source = r#"
namespace MyApp.Models;

public class Order
{
    public int Id { get; set; }
}
"#;

        let file = parse_file(source, "Models/Order.cs");
        assert!(extract(&file).is_none());
    }
}
