use super::common::*;
use crate::context::ParsedFile;
use pecto_core::model::*;

/// Extract Celery tasks, APScheduler jobs, and other scheduled patterns from Python.
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
        && !full_text.contains("add_job")
        && !full_text.contains("APScheduler")
        && !full_text.contains("BlockingScheduler")
        && !full_text.contains("BackgroundScheduler")
        && !full_text.contains("AsyncIOScheduler")
    {
        return None;
    }

    let mut tasks = Vec::new();

    // Extract Celery decorator-based tasks
    for i in 0..root.named_child_count() {
        let node = root.named_child(i).unwrap();

        if node.kind() == "decorated_definition" {
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
    }

    // Extract APScheduler add_job() calls
    if full_text.contains("add_job") {
        extract_apscheduler_jobs(full_text, &mut tasks);
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

/// Extract APScheduler `scheduler.add_job(func, trigger, ...)` patterns via text scanning.
/// Handles multi-line add_job() calls by joining lines between `add_job(` and closing `)`.
fn extract_apscheduler_jobs(source: &str, tasks: &mut Vec<ScheduledTask>) {
    // Collect multi-line add_job() blocks
    let mut blocks = Vec::new();
    let mut current_block = String::new();
    let mut paren_depth = 0i32;
    let mut in_add_job = false;

    for line in source.lines() {
        let trimmed = line.trim();

        if !in_add_job {
            if trimmed.contains("add_job(") {
                in_add_job = true;
                current_block.clear();
                // Count parens from this line onward
                for ch in trimmed.chars() {
                    if ch == '(' {
                        paren_depth += 1;
                    } else if ch == ')' {
                        paren_depth -= 1;
                    }
                }
                current_block.push_str(trimmed);
                if paren_depth <= 0 {
                    blocks.push(current_block.clone());
                    in_add_job = false;
                    paren_depth = 0;
                }
            }
        } else {
            for ch in trimmed.chars() {
                if ch == '(' {
                    paren_depth += 1;
                } else if ch == ')' {
                    paren_depth -= 1;
                }
            }
            current_block.push(' ');
            current_block.push_str(trimmed);
            if paren_depth <= 0 {
                blocks.push(current_block.clone());
                in_add_job = false;
                paren_depth = 0;
            }
        }
    }

    for block in &blocks {
        let after_add_job = match block.split("add_job(").nth(1) {
            Some(s) => s,
            None => continue,
        };

        // Remove only the outermost closing paren (the one matching add_job's open paren)
        // by finding the balanced closing paren from the end
        let inner = strip_outer_closing_paren(after_add_job.trim_end());

        let args: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
        if args.is_empty() {
            continue;
        }

        // First arg is the function reference (e.g. analyst.run_cycle)
        let func_ref = args[0].trim();
        let task_name = func_ref
            .rsplit('.')
            .next()
            .unwrap_or(func_ref)
            .trim_matches('"')
            .trim_matches('\'')
            .to_string();

        // Try to extract the id= or name= kwarg for a better task name
        let display_name = args
            .iter()
            .find_map(|a| extract_simple_kwarg(a, "id"))
            .map(|v| v.trim_matches('"').trim_matches('\'').to_string())
            .unwrap_or(task_name);

        // Try to find schedule info from trigger or keyword args
        let mut schedule = "apscheduler-job".to_string();
        let full_block = block.to_lowercase();

        if full_block.contains("intervaltrigger")
            || full_block.contains("'interval'")
            || full_block.contains("\"interval\"")
        {
            // Look for minutes=, hours=, seconds= anywhere in the block
            for arg in &args {
                if let Some(v) = extract_simple_kwarg(arg, "minutes") {
                    // Check if it's a numeric literal
                    if v.chars().all(|c| c.is_ascii_digit()) {
                        schedule = format!("every {}min", v);
                    } else {
                        schedule = "interval".to_string();
                    }
                    break;
                } else if let Some(v) = extract_simple_kwarg(arg, "hours") {
                    if v.chars().all(|c| c.is_ascii_digit()) {
                        schedule = format!("every {}h", v);
                    } else {
                        schedule = "interval".to_string();
                    }
                    break;
                } else if let Some(v) = extract_simple_kwarg(arg, "seconds") {
                    if v.chars().all(|c| c.is_ascii_digit()) {
                        schedule = format!("every {}s", v);
                    } else {
                        schedule = "interval".to_string();
                    }
                    break;
                }
            }
        } else if full_block.contains("crontrigger")
            || full_block.contains("'cron'")
            || full_block.contains("\"cron\"")
        {
            schedule = "cron".to_string();
        }

        tasks.push(ScheduledTask {
            name: display_name,
            schedule,
            description: Some("APScheduler job".to_string()),
        });
    }
}

/// Strip only the outermost closing paren from an add_job argument list.
/// For `func, IntervalTrigger(hours=1))` → `func, IntervalTrigger(hours=1)`
/// For `func, trigger=IntervalTrigger(hours=1), )` → `func, trigger=IntervalTrigger(hours=1), `
fn strip_outer_closing_paren(s: &str) -> &str {
    // Find the position of the last ')' and remove only that one
    if let Some(pos) = s.rfind(')') {
        // Check if there's another ')' immediately before — if so, we need to be careful
        // Count parens to find the matching one for add_job's opening '('
        let mut depth = 1i32; // We start after add_job(
        for (i, ch) in s.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        return &s[..i];
                    }
                }
                _ => {}
            }
        }
        // Fallback: just strip the last ')'
        &s[..pos]
    } else {
        s
    }
}

fn extract_simple_kwarg<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let pattern = format!("{}=", key);
    let start = text.find(&pattern)? + pattern.len();
    let remaining = &text[start..];
    let end = remaining.find([',', ')', ' ']).unwrap_or(remaining.len());
    let val = remaining[..end].trim();
    if val.is_empty() { None } else { Some(val) }
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

    #[test]
    fn test_apscheduler_multiline() {
        let source = r#"
from apscheduler.schedulers.blocking import BlockingScheduler
from apscheduler.triggers.interval import IntervalTrigger

scheduler = BlockingScheduler()

scheduler.add_job(
    analyst.run_cycle,
    trigger=IntervalTrigger(minutes=30),
    id="agent_cycle",
    name="Agent Cycle",
    max_instances=1,
)

scheduler.add_job(
    portfolio.snapshot,
    trigger=IntervalTrigger(hours=1),
    id="portfolio_snapshot",
    name="Portfolio Snapshot",
)
"#;

        let file = parse_file(source, "scheduler.py");
        let capability = extract(&file).unwrap();

        assert_eq!(capability.scheduled_tasks.len(), 2);
        assert_eq!(capability.scheduled_tasks[0].name, "agent_cycle");
        assert_eq!(capability.scheduled_tasks[0].schedule, "every 30min");
        assert_eq!(capability.scheduled_tasks[1].name, "portfolio_snapshot");
        assert_eq!(capability.scheduled_tasks[1].schedule, "every 1h");
    }

    #[test]
    fn test_apscheduler_compact_no_trailing_comma() {
        // Edge case: compact add_job() without trailing comma — the )) at end
        // must NOT strip the inner ) from IntervalTrigger(hours=1)
        let source = r#"
from apscheduler.schedulers.blocking import BlockingScheduler
from apscheduler.triggers.interval import IntervalTrigger

scheduler = BlockingScheduler()
scheduler.add_job(my_func, IntervalTrigger(hours=2), id="cleanup")
"#;

        let file = parse_file(source, "scheduler.py");
        let capability = extract(&file).unwrap();

        assert_eq!(capability.scheduled_tasks.len(), 1);
        assert_eq!(capability.scheduled_tasks[0].name, "cleanup");
        assert_eq!(capability.scheduled_tasks[0].schedule, "every 2h");
    }
}
