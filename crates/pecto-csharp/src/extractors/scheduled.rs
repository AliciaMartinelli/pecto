use super::common::*;
use crate::context::ParsedFile;
use pecto_core::model::*;
use tree_sitter::Node;

/// Extract scheduled tasks from BackgroundService/IHostedService classes.
pub fn extract(file: &ParsedFile) -> Option<Capability> {
    let root = file.tree.root_node();
    let source = file.source.as_bytes();

    let mut result = None;
    for_each_class(&root, source, &mut |node, src| {
        if result.is_some() {
            return;
        }

        let class_name = get_class_name(node, src);
        let service_type = detect_background_service(node, src);

        if service_type.is_none() {
            return;
        }

        let capability_name = to_kebab_case(&class_name);
        let mut capability = Capability::new(capability_name, file.path.clone());

        if let Some(body) = node.child_by_field_name("body") {
            extract_scheduled_tasks(&body, src, &class_name, &service_type, &mut capability);
        }

        if !capability.scheduled_tasks.is_empty() {
            result = Some(capability);
        }
    });

    result
}

#[derive(Debug)]
enum BackgroundServiceType {
    BackgroundService,
    HostedService,
}

fn detect_background_service(node: &Node, source: &[u8]) -> Option<BackgroundServiceType> {
    for i in 0..node.named_child_count() {
        let child = node.named_child(i).unwrap();
        if child.kind() == "base_list" {
            let text = node_text(&child, source);
            if text.contains("BackgroundService") {
                return Some(BackgroundServiceType::BackgroundService);
            }
            if text.contains("IHostedService") {
                return Some(BackgroundServiceType::HostedService);
            }
        }
    }
    None
}

fn extract_scheduled_tasks(
    body: &Node,
    source: &[u8],
    class_name: &str,
    service_type: &Option<BackgroundServiceType>,
    capability: &mut Capability,
) {
    for i in 0..body.named_child_count() {
        let member = body.named_child(i).unwrap();
        if member.kind() != "method_declaration" {
            continue;
        }

        let method_name = member
            .child_by_field_name("name")
            .map(|n| node_text(&n, source))
            .unwrap_or_default();

        match method_name.as_str() {
            "ExecuteAsync" => {
                let schedule = member
                    .child_by_field_name("body")
                    .and_then(|b| extract_timer_interval(&b, source))
                    .unwrap_or_else(|| "background-service".to_string());

                capability.scheduled_tasks.push(ScheduledTask {
                    name: format!("{}::ExecuteAsync", class_name),
                    schedule,
                    description: Some(format!(
                        "{} background task",
                        match service_type {
                            Some(BackgroundServiceType::BackgroundService) => "BackgroundService",
                            _ => "Hosted service",
                        }
                    )),
                });
            }
            "StartAsync" => {
                if matches!(service_type, Some(BackgroundServiceType::HostedService)) {
                    let schedule = member
                        .child_by_field_name("body")
                        .and_then(|b| extract_timer_interval(&b, source))
                        .unwrap_or_else(|| "hosted-service".to_string());

                    capability.scheduled_tasks.push(ScheduledTask {
                        name: format!("{}::StartAsync", class_name),
                        schedule,
                        description: Some("IHostedService startup task".to_string()),
                    });
                }
            }
            _ => {}
        }
    }
}

/// Extract timer interval from method body text.
fn extract_timer_interval(body: &Node, source: &[u8]) -> Option<String> {
    let text = node_text(body, source);

    // TimeSpan.FromSeconds(X)
    if let Some(val) = extract_timespan(&text, "FromSeconds") {
        return Some(format!("every {}s", val));
    }
    if let Some(val) = extract_timespan(&text, "FromMinutes") {
        return Some(format!("every {}min", val));
    }
    if let Some(val) = extract_timespan(&text, "FromHours") {
        return Some(format!("every {}h", val));
    }
    if let Some(val) = extract_timespan(&text, "FromMilliseconds") {
        return Some(format!("every {}ms", val));
    }

    // PeriodicTimer pattern
    if text.contains("PeriodicTimer")
        && let Some(val) = extract_timespan(&text, "FromSeconds")
            .or_else(|| extract_timespan(&text, "FromMinutes"))
    {
        return Some(format!("periodic: {}", val));
    }

    // Task.Delay pattern
    if text.contains("Task.Delay")
        && let Some(val) = extract_timespan(&text, "FromSeconds")
            .or_else(|| extract_timespan(&text, "FromMinutes"))
    {
        return Some(format!("polling: {}", val));
    }

    None
}

fn extract_timespan(text: &str, method: &str) -> Option<String> {
    let pattern = format!("{}(", method);
    let start = text.find(&pattern)? + pattern.len();
    let end = text[start..].find(')')? + start;
    let val = text[start..end].trim();
    if val.is_empty() {
        return None;
    }
    Some(val.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ParsedFile;

    fn parse_file(source: &str, path: &str) -> ParsedFile {
        ParsedFile::parse(source.to_string(), path.to_string()).unwrap()
    }

    #[test]
    fn test_background_service_with_timer() {
        let source = r#"
namespace MyApp.Workers;

public class CleanupWorker : BackgroundService
{
    protected override async Task ExecuteAsync(CancellationToken stoppingToken)
    {
        while (!stoppingToken.IsCancellationRequested)
        {
            await DoCleanup();
            await Task.Delay(TimeSpan.FromMinutes(5), stoppingToken);
        }
    }
}
"#;

        let file = parse_file(source, "Workers/CleanupWorker.cs");
        let capability = extract(&file).unwrap();

        assert_eq!(capability.scheduled_tasks.len(), 1);
        let task = &capability.scheduled_tasks[0];
        assert!(task.name.contains("CleanupWorker"));
        assert!(task.schedule.contains("5"));
        assert!(
            task.description
                .as_ref()
                .unwrap()
                .contains("BackgroundService")
        );
    }

    #[test]
    fn test_hosted_service() {
        let source = r#"
namespace MyApp.Workers;

public class HealthCheckService : IHostedService
{
    public Task StartAsync(CancellationToken cancellationToken)
    {
        _timer = new Timer(DoWork, null, TimeSpan.FromSeconds(0), TimeSpan.FromSeconds(30));
        return Task.CompletedTask;
    }

    public Task StopAsync(CancellationToken cancellationToken)
    {
        return Task.CompletedTask;
    }
}
"#;

        let file = parse_file(source, "Workers/HealthCheckService.cs");
        let capability = extract(&file).unwrap();

        assert_eq!(capability.scheduled_tasks.len(), 1);
        let task = &capability.scheduled_tasks[0];
        assert!(task.name.contains("HealthCheckService"));
        // Timer has two TimeSpan args — schedule should contain a time value
        assert!(
            task.schedule.contains("s") || task.schedule.contains("hosted"),
            "Schedule should contain interval info, got: {}",
            task.schedule
        );
    }

    #[test]
    fn test_non_background_class() {
        let source = r#"
namespace MyApp.Services;

public class OrderService
{
    public void Process() { }
}
"#;

        let file = parse_file(source, "Services/OrderService.cs");
        assert!(extract(&file).is_none());
    }
}
