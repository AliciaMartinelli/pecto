use super::common::*;
use crate::context::ParsedFile;
use pecto_core::model::*;

/// Extract scheduled tasks and event listeners from a parsed file.
/// These are added to an existing capability if present, or create a new one.
pub fn extract(file: &ParsedFile) -> Option<Capability> {
    let root = file.tree.root_node();
    let source = file.source.as_bytes();

    for i in 0..root.named_child_count() {
        let node = root.named_child(i).unwrap();
        if node.kind() != "class_declaration" {
            continue;
        }

        let class_name = get_class_name(&node, source);
        let mut scheduled_tasks = Vec::new();

        let body = match node.child_by_field_name("body") {
            Some(b) => b,
            None => continue,
        };

        for j in 0..body.named_child_count() {
            let member = body.named_child(j).unwrap();
            if member.kind() != "method_declaration" {
                continue;
            }

            let annotations = collect_annotations(&member, source);

            // @Scheduled tasks
            if let Some(task) = extract_scheduled_task(&member, source, &annotations) {
                scheduled_tasks.push(task);
            }

            // @EventListener methods — represented as scheduled tasks with "event:" prefix
            if let Some(task) = extract_event_listener(&member, source, &annotations) {
                scheduled_tasks.push(task);
            }
        }

        if !scheduled_tasks.is_empty() {
            let capability_name = to_kebab_case(&class_name);
            let mut capability = Capability::new(capability_name, file.path.clone());
            capability.scheduled_tasks = scheduled_tasks;
            return Some(capability);
        }
    }

    None
}

fn extract_scheduled_task(
    method: &tree_sitter::Node,
    source: &[u8],
    annotations: &[AnnotationInfo],
) -> Option<ScheduledTask> {
    let ann = annotations.iter().find(|a| a.name == "Scheduled")?;

    let method_name = method
        .child_by_field_name("name")
        .map(|n| node_text(&n, source))
        .unwrap_or_default();

    let schedule = if let Some(cron) = ann.arguments.get("cron") {
        format!("cron: {}", cron)
    } else if let Some(rate) = ann.arguments.get("fixedRate") {
        format!("every {}ms", rate)
    } else if let Some(delay) = ann.arguments.get("fixedDelay") {
        format!("fixedDelay: {}ms", delay)
    } else if let Some(rate_str) = ann.arguments.get("fixedRateString") {
        format!("every {}", rate_str)
    } else {
        "unknown schedule".to_string()
    };

    Some(ScheduledTask {
        name: method_name,
        schedule,
        description: None,
    })
}

fn extract_event_listener(
    method: &tree_sitter::Node,
    source: &[u8],
    annotations: &[AnnotationInfo],
) -> Option<ScheduledTask> {
    let ann = annotations.iter().find(|a| a.name == "EventListener")?;

    let method_name = method
        .child_by_field_name("name")
        .map(|n| node_text(&n, source))
        .unwrap_or_default();

    // Get event type from annotation value or first parameter type
    let event_type = ann
        .value
        .as_ref()
        .map(|v| v.trim_end_matches(".class").to_string())
        .or_else(|| extract_first_param_type(method, source));

    let schedule = format!("event: {}", event_type.as_deref().unwrap_or("unknown"));

    Some(ScheduledTask {
        name: method_name,
        schedule,
        description: event_type.map(|t| format!("Handles {} events", t)),
    })
}

fn extract_first_param_type(method: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    let params = method.child_by_field_name("parameters")?;
    for i in 0..params.named_child_count() {
        let param = params.named_child(i).unwrap();
        if param.kind() == "formal_parameter" {
            return param
                .child_by_field_name("type")
                .map(|t| node_text(&t, source));
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
    fn test_scheduled_tasks() {
        let source = r#"
@Component
public class ScheduledJobs {

    @Scheduled(cron = "0 0 * * * *")
    public void hourlyCleanup() {
        tempFileService.cleanOldFiles();
    }

    @Scheduled(fixedRate = 60000)
    public void checkHealth() {
        healthService.ping();
    }

    @Scheduled(fixedDelay = 30000)
    public void processQueue() {
        queueService.processNext();
    }
}
"#;

        let file = parse_file(source, "src/ScheduledJobs.java");
        let capability = extract(&file).unwrap();

        assert_eq!(capability.scheduled_tasks.len(), 3);

        let cleanup = &capability.scheduled_tasks[0];
        assert_eq!(cleanup.name, "hourlyCleanup");
        assert!(cleanup.schedule.contains("cron:"));

        let health = &capability.scheduled_tasks[1];
        assert_eq!(health.name, "checkHealth");
        assert!(health.schedule.contains("60000"));

        let queue = &capability.scheduled_tasks[2];
        assert_eq!(queue.name, "processQueue");
        assert!(queue.schedule.contains("fixedDelay"));
    }

    #[test]
    fn test_event_listeners() {
        let source = r#"
@Component
public class OrderEventHandler {

    @EventListener(OrderCreatedEvent.class)
    public void handleOrderCreated(OrderCreatedEvent event) {
        notificationService.sendConfirmation(event.getOrderId());
    }

    @EventListener
    public void handlePaymentCompleted(PaymentCompletedEvent event) {
        orderService.markAsPaid(event.getOrderId());
    }
}
"#;

        let file = parse_file(source, "src/OrderEventHandler.java");
        let capability = extract(&file).unwrap();

        assert_eq!(capability.scheduled_tasks.len(), 2);

        let order_handler = &capability.scheduled_tasks[0];
        assert_eq!(order_handler.name, "handleOrderCreated");
        assert!(order_handler.schedule.contains("event: OrderCreatedEvent"));
        assert!(
            order_handler
                .description
                .as_ref()
                .unwrap()
                .contains("OrderCreatedEvent")
        );

        let payment_handler = &capability.scheduled_tasks[1];
        assert_eq!(payment_handler.name, "handlePaymentCompleted");
        assert!(
            payment_handler
                .schedule
                .contains("event: PaymentCompletedEvent")
        );
    }

    #[test]
    fn test_no_scheduled_tasks() {
        let source = r#"
public class PlainService {
    public void doSomething() {}
}
"#;

        let file = parse_file(source, "src/PlainService.java");
        assert!(extract(&file).is_none());
    }
}
