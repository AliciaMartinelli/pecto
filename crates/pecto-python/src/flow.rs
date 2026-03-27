use crate::context::AnalysisContext;
use crate::extractors::common::*;
use pecto_core::model::*;

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

            let method_source = find_endpoint_function_text(
                &file.tree.root_node(), file.source.as_bytes(), endpoint,
            ).unwrap_or_else(|| file.source.clone());
            trace_source_text(&method_source, &mut steps);

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

/// Find the source text of the specific function handling a Python endpoint.
fn find_endpoint_function_text(
    root: &tree_sitter::Node,
    source: &[u8],
    endpoint: &Endpoint,
) -> Option<String> {
    let method_lower = format!("{:?}", endpoint.method).to_lowercase();

    // Walk decorated_definition nodes
    for i in 0..root.named_child_count() {
        let node = root.named_child(i).unwrap();
        if node.kind() == "decorated_definition" {
            let decorators = collect_decorators(&node, source);
            for dec in &decorators {
                // Match decorator method: @router.get, @app.post, etc.
                let dec_method = dec.name.to_lowercase();
                if dec_method == method_lower
                    || dec.full_name.to_lowercase().ends_with(&format!(".{}", method_lower))
                {
                    // Check path match if decorator has args
                    if let Some(path_arg) = dec.args.first() {
                        let clean_path = clean_string_literal(path_arg);
                        if !endpoint.path.ends_with(&clean_path) && !clean_path.is_empty() {
                            continue;
                        }
                    }
                    // Found matching decorator — extract function body text
                    if let Some(func) = node.child_by_field_name("definition") {
                        if let Some(body) = func.child_by_field_name("body") {
                            return Some(node_text(&body, source));
                        }
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
