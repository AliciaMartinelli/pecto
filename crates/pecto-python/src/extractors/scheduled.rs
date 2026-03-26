use super::common::*;
use crate::context::ParsedFile;
use pecto_core::model::*;

/// Extract Celery tasks and scheduled jobs from Python.
pub fn extract(file: &ParsedFile) -> Option<Capability> {
    let root = file.tree.root_node();
    let source = file.source.as_bytes();
    let full_text = &file.source;

    // Quick check
    if !full_text.contains("@app.task")
        && !full_text.contains("@shared_task")
        && !full_text.contains("@periodic_task")
        && !full_text.contains("@celery")
        && !full_text.contains("crontab")
    {
        return None;
    }

    let mut tasks = Vec::new();

    for i in 0..root.named_child_count() {
        let node = root.named_child(i).unwrap();

        if node.kind() != "decorated_definition" {
            continue;
        }

        let decorators = collect_decorators(&node, source);
        let inner = match get_inner_definition(&node) {
            Some(n) if n.kind() == "function_definition" => n,
            _ => continue,
        };

        let is_task = decorators.iter().any(|d| {
            d.name == "task"
                || d.name == "shared_task"
                || d.name == "periodic_task"
                || d.full_name.contains("celery")
                || d.full_name.contains("app.task")
        });

        if !is_task {
            continue;
        }

        let func_name = get_def_name(&inner, source);

        // Try to extract schedule from periodic_task decorator
        let schedule = decorators
            .iter()
            .find(|d| d.name == "periodic_task")
            .and_then(|d| {
                d.args
                    .iter()
                    .find(|a| a.contains("crontab") || a.contains("schedule"))
                    .cloned()
            })
            .unwrap_or_else(|| "celery-task".to_string());

        tasks.push(ScheduledTask {
            name: func_name,
            schedule: clean_string_literal(&schedule),
            description: Some("Celery task".to_string()),
        });
    }

    if tasks.is_empty() {
        return None;
    }

    let file_stem = file
        .path
        .rsplit('/')
        .next()
        .unwrap_or(&file.path)
        .trim_end_matches(".py");
    let capability_name = to_kebab_case(file_stem);

    let mut capability = Capability::new(capability_name, file.path.clone());
    capability.scheduled_tasks = tasks;
    Some(capability)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ParsedFile;

    fn parse_file(source: &str, path: &str) -> ParsedFile {
        ParsedFile::parse(source.to_string(), path.to_string()).unwrap()
    }

    #[test]
    fn test_celery_task() {
        let source = r#"
from celery import shared_task

@shared_task
def send_email(to: str, subject: str):
    mail.send(to, subject)

@shared_task
def process_order(order_id: int):
    Order.process(order_id)
"#;

        let file = parse_file(source, "tasks/email_tasks.py");
        let capability = extract(&file).unwrap();

        assert_eq!(capability.scheduled_tasks.len(), 2);
        assert_eq!(capability.scheduled_tasks[0].name, "send_email");
        assert_eq!(capability.scheduled_tasks[1].name, "process_order");
    }

    #[test]
    fn test_no_tasks() {
        let source = r#"
def regular_function():
    pass
"#;
        let file = parse_file(source, "utils.py");
        assert!(extract(&file).is_none());
    }
}
