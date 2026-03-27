use crate::context::AnalysisContext;
use crate::extractors::common::*;
use pecto_core::model::*;

/// Extract request flows for C# endpoints.
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

            // Security
            if let Some(sec) = &endpoint.security
                && sec.authentication.is_some()
            {
                steps.push(FlowStep {
                    actor: cap.name.clone(),
                    method: "auth".to_string(),
                    kind: FlowStepKind::SecurityGuard,
                    description: "Auth required".to_string(),
                    condition: None,
                    children: Vec::new(),
                });
            }

            // Trace only the specific endpoint method body
            let method_source = find_endpoint_method_text(
                &file.tree.root_node(), file.source.as_bytes(), endpoint,
            ).unwrap_or_else(|| file.source.clone());
            trace_source_text(&method_source, &mut steps);

            // Return
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

/// Find the source text of the specific method handling a C# endpoint.
fn find_endpoint_method_text(
    root: &tree_sitter::Node,
    source: &[u8],
    endpoint: &Endpoint,
) -> Option<String> {
    let http_attr = match endpoint.method {
        HttpMethod::Get => "HttpGet",
        HttpMethod::Post => "HttpPost",
        HttpMethod::Put => "HttpPut",
        HttpMethod::Delete => "HttpDelete",
        HttpMethod::Patch => "HttpPatch",
    };

    for_each_node(root, "class_declaration", &mut |class_node| {
        if let Some(body) = class_node.child_by_field_name("body") {
            for i in 0..body.named_child_count() {
                let member = body.named_child(i).unwrap();
                if member.kind() == "method_declaration" {
                    let attrs = collect_attributes(&member, source);
                    if attrs.iter().any(|a| a.name == http_attr) {
                        if let Some(method_body) = member.child_by_field_name("body") {
                            return Some(node_text(&method_body, source));
                        }
                    }
                }
            }
        }
        None
    })
}

/// Walk tree-sitter nodes to find matching node types.
fn for_each_node<F>(node: &tree_sitter::Node, kind: &str, callback: &mut F) -> Option<String>
where
    F: FnMut(&tree_sitter::Node) -> Option<String>,
{
    if node.kind() == kind {
        if let Some(result) = callback(node) {
            return Some(result);
        }
    }
    for i in 0..node.named_child_count() {
        let child = node.named_child(i).unwrap();
        if let Some(result) = for_each_node(&child, kind, callback) {
            return Some(result);
        }
    }
    None
}

fn trace_source_text(source: &str, steps: &mut Vec<FlowStep>) {
    for line in source.lines() {
        let trimmed = line.trim();

        if trimmed.contains(".Add(") || trimmed.contains(".SaveChangesAsync") {
            steps.push(FlowStep {
                actor: "DbContext".to_string(),
                method: "save".to_string(),
                kind: FlowStepKind::DbWrite,
                description: "DB write via EF Core".to_string(),
                condition: None,
                children: Vec::new(),
            });
        } else if trimmed.contains(".Remove(") {
            steps.push(FlowStep {
                actor: "DbContext".to_string(),
                method: "delete".to_string(),
                kind: FlowStepKind::DbWrite,
                description: "DB delete via EF Core".to_string(),
                condition: None,
                children: Vec::new(),
            });
        } else if trimmed.contains(".Publish(") || trimmed.contains(".Send(") {
            steps.push(FlowStep {
                actor: "Mediator".to_string(),
                method: "publish".to_string(),
                kind: FlowStepKind::EventPublish,
                description: "Publish event".to_string(),
                condition: None,
                children: Vec::new(),
            });
        }
    }
}
