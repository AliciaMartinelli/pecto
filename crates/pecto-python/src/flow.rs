use crate::context::AnalysisContext;
use crate::extractors::common::*;
use pecto_core::model::*;

const MAX_DEPTH: usize = 4;

/// Extract request flows for Python endpoints.
pub fn extract_flows(spec: &mut ProjectSpec, ctx: &AnalysisContext) {
    let mut flows = Vec::new();

    for cap in &spec.capabilities {
        for endpoint in &cap.endpoints {
            let trigger = format!("{:?} {}", endpoint.method, endpoint.path);
            let entry_point = format!("{}#{}", cap.source, cap.name);

            let Some(file) = ctx.files.iter().find(|f| f.path == cap.source) else {
                continue;
            };

            let root = file.tree.root_node();
            let source = file.source.as_bytes();
            let mut steps = Vec::new();

            // Security (Depends)
            if let Some(sec) = &endpoint.security
                && sec.authentication.is_some()
            {
                steps.push(FlowStep {
                    actor: cap.name.clone(),
                    method: "auth".to_string(),
                    kind: FlowStepKind::SecurityGuard,
                    description: "Depends(auth)".to_string(),
                    condition: None,
                    children: Vec::new(),
                });
            }

            // Trace via AST
            let method_steps =
                if let Some(func_body) = find_endpoint_function_body(&root, source, endpoint) {
                    trace_function_body(&func_body, source, 0)
                } else {
                    // Fallback: text-based
                    let method_source = find_endpoint_function_text(&root, source, endpoint)
                        .unwrap_or_else(|| file.source.clone());
                    let mut fallback = Vec::new();
                    trace_source_text(&method_source, &mut fallback);
                    fallback
                };
            steps.extend(method_steps);

            if let Some(b) = endpoint.behaviors.first() {
                steps.push(FlowStep {
                    actor: cap.name.clone(),
                    method: "return".to_string(),
                    kind: FlowStepKind::Return,
                    description: format!("Return: {}", b.returns.status),
                    condition: None,
                    children: Vec::new(),
                });
            }

            if steps.len() > 1 {
                flows.push(RequestFlow {
                    trigger,
                    entry_point,
                    steps,
                });
            }
        }
    }

    spec.flows = flows;
}

// ==================== AST-based Tracing ====================

/// Find the function body AST node for a Python endpoint.
fn find_endpoint_function_body<'a>(
    root: &'a tree_sitter::Node<'a>,
    source: &[u8],
    endpoint: &Endpoint,
) -> Option<tree_sitter::Node<'a>> {
    let method_lower = format!("{:?}", endpoint.method).to_lowercase();

    for i in 0..root.named_child_count() {
        let node = root.named_child(i).unwrap();
        if node.kind() == "decorated_definition" {
            let decorators = collect_decorators(&node, source);
            for dec in &decorators {
                let dec_method = dec.name.to_lowercase();
                if dec_method == method_lower
                    || dec.full_name.to_lowercase().ends_with(&format!(".{}", method_lower))
                {
                    if let Some(path_arg) = dec.args.first() {
                        let clean_path = clean_string_literal(path_arg);
                        if !endpoint.path.ends_with(&clean_path) && !clean_path.is_empty() {
                            continue;
                        }
                    }
                    if let Some(func) = node.child_by_field_name("definition") {
                        return func.child_by_field_name("body");
                    }
                }
            }
        }
    }
    None
}

fn trace_function_body(
    body: &tree_sitter::Node,
    source: &[u8],
    depth: usize,
) -> Vec<FlowStep> {
    let mut steps = Vec::new();
    if depth >= MAX_DEPTH {
        return steps;
    }
    // Python functions are top-level, no class body context needed for most cases
    // For class-based views, we'd need class body — but FastAPI/Flask use functions
    trace_node_recursive(body, source, depth, &mut steps);
    steps
}

fn trace_node_recursive(
    node: &tree_sitter::Node,
    source: &[u8],
    depth: usize,
    steps: &mut Vec<FlowStep>,
) {
    if depth >= MAX_DEPTH {
        return;
    }
    match node.kind() {
        "call" => {
            let text = node_text(node, source);

            // Classify the call
            if let Some(step) = classify_method_call(&text) {
                steps.push(step);
                return;
            }
        }
        "raise_statement" => {
            let text = node_text(node, source);
            let exception = text
                .strip_prefix("raise ")
                .and_then(|s| s.split('(').next())
                .unwrap_or("Exception")
                .trim();
            steps.push(FlowStep {
                actor: "".to_string(),
                method: "raise".to_string(),
                kind: FlowStepKind::ThrowException,
                description: format!("raise {}", exception),
                condition: None,
                children: Vec::new(),
            });
            return;
        }
        "if_statement" => {
            let condition_text = node
                .child_by_field_name("condition")
                .map(|c| node_text(&c, source))
                .unwrap_or_default();

            let mut if_children = Vec::new();
            if let Some(consequence) = node.child_by_field_name("consequence") {
                trace_node_recursive(&consequence, source, depth + 1, &mut if_children);
            }

            let mut else_children = Vec::new();
            if let Some(alternative) = node.child_by_field_name("alternative") {
                trace_node_recursive(&alternative, source, depth + 1, &mut else_children);
            }

            if !if_children.is_empty() || !else_children.is_empty() {
                steps.push(FlowStep {
                    actor: "".to_string(),
                    method: "if".to_string(),
                    kind: FlowStepKind::Condition,
                    description: format!("if {}", condition_text),
                    condition: Some(condition_text),
                    children: if_children,
                });
                if !else_children.is_empty() {
                    steps.push(FlowStep {
                        actor: "".to_string(),
                        method: "else".to_string(),
                        kind: FlowStepKind::Condition,
                        description: "else".to_string(),
                        condition: Some("else".to_string()),
                        children: else_children,
                    });
                }
                return;
            }
        }
        _ => {}
    }

    for i in 0..node.child_count() {
        let child = node.child(i).unwrap();
        trace_node_recursive(&child, source, depth, steps);
    }
}

fn classify_method_call(text: &str) -> Option<FlowStep> {
    // DB writes (SQLAlchemy, Django ORM)
    if text.contains(".save(")
        || text.contains(".add(")
        || text.contains(".commit(")
        || text.contains(".bulk_create(")
        || text.contains(".bulk_update(")
    {
        let target = text.split('.').next().unwrap_or("db").trim();
        return Some(FlowStep {
            actor: if target == "db" || target == "session" {
                "DB".to_string()
            } else {
                target.to_string()
            },
            method: "save".to_string(),
            kind: FlowStepKind::DbWrite,
            description: format!("DB write: {}", target),
            condition: None,
            children: Vec::new(),
        });
    }

    if text.contains(".delete(") && !text.contains("request.") {
        let target = text.split('.').next().unwrap_or("").trim();
        return Some(FlowStep {
            actor: target.to_string(),
            method: "delete".to_string(),
            kind: FlowStepKind::DbWrite,
            description: format!("DB delete: {}", target),
            condition: None,
            children: Vec::new(),
        });
    }

    // DB reads
    if text.contains(".query(")
        || text.contains(".filter(")
        || text.contains(".get(") && text.contains(".objects")
        || text.contains(".all(")
        || text.contains(".first(")
        || text.contains(".execute(")
    {
        let target = text.split('.').next().unwrap_or("db").trim();
        return Some(FlowStep {
            actor: if target == "db" || target == "session" {
                "DB".to_string()
            } else {
                target.to_string()
            },
            method: "query".to_string(),
            kind: FlowStepKind::DbRead,
            description: format!("DB read: {}", target),
            condition: None,
            children: Vec::new(),
        });
    }

    // Events (Celery, event bus)
    if text.contains(".publish(")
        || text.contains("send_task(")
        || text.contains(".delay(")
        || text.contains(".apply_async(")
    {
        return Some(FlowStep {
            actor: "EventBus".to_string(),
            method: "publish".to_string(),
            kind: FlowStepKind::EventPublish,
            description: "Publish event".to_string(),
            condition: None,
            children: Vec::new(),
        });
    }

    // Service calls: service.method() or self.service.method()
    if text.contains('.') && text.contains('(') {
        let clean = text.trim_start_matches("await ");
        let parts: Vec<&str> = clean.splitn(2, '.').collect();
        if parts.len() == 2 {
            let target = parts[0].trim();
            let rest = parts[1];
            let method = rest.split('(').next().unwrap_or("").trim();

            // self.service.method() → service.method()
            if target == "self" && rest.contains('.') {
                let inner: Vec<&str> = rest.splitn(2, '.').collect();
                if inner.len() == 2 {
                    let svc = inner[0].trim();
                    let svc_method = inner[1].split('(').next().unwrap_or("").trim();
                    if !svc.is_empty()
                        && svc.chars().next().is_some_and(|c| c.is_lowercase())
                        && !is_excluded_py_target(svc)
                        && !is_excluded_py_method(svc_method)
                    {
                        return Some(FlowStep {
                            actor: svc.to_string(),
                            method: svc_method.to_string(),
                            kind: FlowStepKind::ServiceCall,
                            description: format!("Call: {}.{}()", svc, svc_method),
                            condition: None,
                            children: Vec::new(),
                        });
                    }
                }
            }

            if !target.is_empty()
                && target.chars().next().is_some_and(|c| c.is_lowercase())
                && !is_excluded_py_target(target)
                && !is_excluded_py_method(method)
            {
                return Some(FlowStep {
                    actor: target.to_string(),
                    method: method.to_string(),
                    kind: FlowStepKind::ServiceCall,
                    description: format!("Call: {}.{}()", target, method),
                    condition: None,
                    children: Vec::new(),
                });
            }
        }
    }

    None
}

fn is_excluded_py_target(target: &str) -> bool {
    matches!(
        target,
        "self"
            | "cls"
            | "super"
            | "print"
            | "len"
            | "range"
            | "str"
            | "int"
            | "float"
            | "bool"
            | "list"
            | "dict"
            | "set"
            | "tuple"
            | "type"
            | "isinstance"
            | "logger"
            | "logging"
            | "os"
            | "sys"
            | "json"
            | "re"
            | "request"
            | "response"
    )
}

fn is_excluded_py_method(method: &str) -> bool {
    matches!(
        method,
        "append"
            | "extend"
            | "insert"
            | "pop"
            | "remove"
            | "sort"
            | "reverse"
            | "keys"
            | "values"
            | "items"
            | "get"
            | "update"
            | "format"
            | "strip"
            | "split"
            | "join"
            | "replace"
            | "lower"
            | "upper"
            | "startswith"
            | "endswith"
            | "encode"
            | "decode"
            | "info"
            | "debug"
            | "warning"
            | "error"
            | "exception"
            | "critical"
    )
}

// ==================== Text-based Fallback ====================

fn find_endpoint_function_text(
    root: &tree_sitter::Node,
    source: &[u8],
    endpoint: &Endpoint,
) -> Option<String> {
    let method_lower = format!("{:?}", endpoint.method).to_lowercase();

    for i in 0..root.named_child_count() {
        let node = root.named_child(i).unwrap();
        if node.kind() == "decorated_definition" {
            let decorators = collect_decorators(&node, source);
            for dec in &decorators {
                let dec_method = dec.name.to_lowercase();
                if dec_method == method_lower
                    || dec.full_name.to_lowercase().ends_with(&format!(".{}", method_lower))
                {
                    if let Some(path_arg) = dec.args.first() {
                        let clean_path = clean_string_literal(path_arg);
                        if !endpoint.path.ends_with(&clean_path) && !clean_path.is_empty() {
                            continue;
                        }
                    }
                    if let Some(func) = node.child_by_field_name("definition")
                        && let Some(body) = func.child_by_field_name("body") {
                            return Some(node_text(&body, source));
                        }
                }
            }
        }
    }
    None
}

fn trace_source_text(source: &str, steps: &mut Vec<FlowStep>) {
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.contains(".save(") || trimmed.contains(".add(") || trimmed.contains(".commit(") {
            steps.push(FlowStep {
                actor: "DB".to_string(),
                method: "save".to_string(),
                kind: FlowStepKind::DbWrite,
                description: "DB write".to_string(),
                condition: None,
                children: Vec::new(),
            });
        } else if trimmed.contains(".query(")
            || trimmed.contains(".filter(") && trimmed.contains(".all()")
        {
            steps.push(FlowStep {
                actor: "DB".to_string(),
                method: "query".to_string(),
                kind: FlowStepKind::DbRead,
                description: "DB query".to_string(),
                condition: None,
                children: Vec::new(),
            });
        } else if trimmed.contains("publish(") || trimmed.contains("send_task(") {
            steps.push(FlowStep {
                actor: "EventBus".to_string(),
                method: "publish".to_string(),
                kind: FlowStepKind::EventPublish,
                description: "Publish event".to_string(),
                condition: None,
                children: Vec::new(),
            });
        }
    }
}
