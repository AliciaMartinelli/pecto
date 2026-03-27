use crate::context::AnalysisContext;
use crate::extractors::common::*;
use pecto_core::model::*;

const MAX_DEPTH: usize = 4;

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

            let root = file.tree.root_node();
            let source = file.source.as_bytes();
            let mut steps = Vec::new();

            // Security
            if let Some(sec) = &endpoint.security
                && sec.authentication.is_some()
            {
                steps.push(FlowStep {
                    actor: cap.name.clone(),
                    method: "auth".to_string(),
                    kind: FlowStepKind::SecurityGuard,
                    description: if sec.roles.is_empty() {
                        "Auth required".to_string()
                    } else {
                        format!("Auth: {}", sec.roles.join(", "))
                    },
                    condition: None,
                    children: Vec::new(),
                });
            }

            // Trace via AST
            let method_steps =
                if let Some(method_body) = find_endpoint_method_body(&root, source, endpoint) {
                    trace_method_body(&method_body, source, 0)
                } else {
                    // Fallback: text-based
                    let method_source = find_endpoint_method_text(&root, source, endpoint)
                        .unwrap_or_else(|| file.source.clone());
                    let mut fallback = Vec::new();
                    trace_source_text(&method_source, &mut fallback);
                    fallback
                };
            steps.extend(method_steps);

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

// ==================== AST-based Tracing ====================

/// Find the method body AST node for a C# endpoint.
/// Searches through namespace/class nesting up to 3 levels deep.
fn find_endpoint_method_body<'a>(
    root: &'a tree_sitter::Node<'a>,
    source: &[u8],
    endpoint: &Endpoint,
) -> Option<tree_sitter::Node<'a>> {
    let http_attr = match endpoint.method {
        HttpMethod::Get => "HttpGet",
        HttpMethod::Post => "HttpPost",
        HttpMethod::Put => "HttpPut",
        HttpMethod::Delete => "HttpDelete",
        HttpMethod::Patch => "HttpPatch",
    };

    // Collect class_declaration nodes (may be nested in namespaces)
    let mut class_nodes = Vec::new();
    for i in 0..root.named_child_count() {
        let l1 = root.named_child(i).unwrap();
        if l1.kind() == "class_declaration" {
            class_nodes.push(l1);
        }
        for j in 0..l1.named_child_count() {
            let l2 = l1.named_child(j).unwrap();
            if l2.kind() == "class_declaration" {
                class_nodes.push(l2);
            }
            for k in 0..l2.named_child_count() {
                let l3 = l2.named_child(k).unwrap();
                if l3.kind() == "class_declaration" {
                    class_nodes.push(l3);
                }
            }
        }
    }

    // Search each class for matching method
    for class_node in class_nodes {
        if let Some(body) = class_node.child_by_field_name("body") {
            for i in 0..body.named_child_count() {
                let member = body.named_child(i).unwrap();
                if member.kind() == "method_declaration" {
                    let attrs = collect_attributes(&member, source);
                    if attrs.iter().any(|a| a.name == http_attr) {
                        return member.child_by_field_name("body");
                    }
                }
            }
        }
    }
    None
}

fn trace_method_body(
    body: &tree_sitter::Node,
    source: &[u8],
    depth: usize,
) -> Vec<FlowStep> {
    let class_body = find_enclosing_class_body(body);
    let mut steps = Vec::new();
    if depth >= MAX_DEPTH {
        return steps;
    }
    trace_node_recursive(body, source, depth, &mut steps, class_body.as_ref());
    steps
}

fn find_enclosing_class_body<'a>(
    node: &'a tree_sitter::Node<'a>,
) -> Option<tree_sitter::Node<'a>> {
    let mut current = node.parent();
    while let Some(n) = current {
        if n.kind() == "class_declaration" {
            return n.child_by_field_name("body");
        }
        current = n.parent();
    }
    None
}

fn find_method_in_class<'a>(
    class_body: &'a tree_sitter::Node<'a>,
    method_name: &str,
    source: &[u8],
) -> Option<tree_sitter::Node<'a>> {
    for i in 0..class_body.named_child_count() {
        let member = class_body.named_child(i).unwrap();
        if member.kind() == "method_declaration"
            && let Some(name_node) = member.child_by_field_name("name")
                && node_text(&name_node, source) == method_name {
                    return member.child_by_field_name("body");
                }
    }
    None
}

fn trace_node_recursive(
    node: &tree_sitter::Node,
    source: &[u8],
    depth: usize,
    steps: &mut Vec<FlowStep>,
    class_body: Option<&tree_sitter::Node>,
) {
    match node.kind() {
        "invocation_expression" => {
            let text = node_text(node, source);

            // Check for bare method calls (internal)
            if let Some(expr) = node.child_by_field_name("function") {
                if expr.kind() == "identifier" {
                    let method_name = node_text(&expr, source);
                    if let Some(cb) = class_body
                        && depth < MAX_DEPTH
                            && let Some(target_body) =
                                find_method_in_class(cb, &method_name, source)
                            {
                                trace_node_recursive(
                                    &target_body, source, depth + 1, steps, Some(cb),
                                );
                                return;
                            }
                }

                // _service.Method() or service.Method()
                if expr.kind() == "member_access_expression"
                    && let Some(obj) = expr.child_by_field_name("expression") {
                        let obj_text = node_text(&obj, source);
                        if let Some(name) = expr.child_by_field_name("name") {
                            let method_name = node_text(&name, source);

                            // Await expressions: unwrap
                            let actual_obj = obj_text.trim_start_matches("await ");

                            if actual_obj == "this" {
                                // this.Method() → resolve internally
                                if let Some(cb) = class_body
                                    && depth < MAX_DEPTH
                                        && let Some(target_body) =
                                            find_method_in_class(cb, &method_name, source)
                                        {
                                            trace_node_recursive(
                                                &target_body, source, depth + 1, steps, Some(cb),
                                            );
                                            return;
                                        }
                            }
                        }
                    }
            }

            if let Some(step) = classify_method_call(&text) {
                steps.push(step);
                return;
            }
        }
        "throw_statement" | "throw_expression" => {
            let text = node_text(node, source);
            let exception = text
                .split("new ")
                .nth(1)
                .and_then(|s| s.split('(').next())
                .unwrap_or("Exception")
                .trim();
            steps.push(FlowStep {
                actor: "".to_string(),
                method: "throw".to_string(),
                kind: FlowStepKind::ThrowException,
                description: format!("throw {}", exception),
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
                trace_node_recursive(&consequence, source, depth + 1, &mut if_children, class_body);
            }

            let mut else_children = Vec::new();
            if let Some(alternative) = node.child_by_field_name("alternative") {
                trace_node_recursive(
                    &alternative, source, depth + 1, &mut else_children, class_body,
                );
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
        trace_node_recursive(&child, source, depth, steps, class_body);
    }
}

fn classify_method_call(text: &str) -> Option<FlowStep> {
    // EF Core DB writes
    if text.contains(".Add(")
        || text.contains(".AddAsync(")
        || text.contains(".SaveChanges")
        || text.contains(".Update(")
    {
        let target = text.split('.').next().unwrap_or("").trim();
        return Some(FlowStep {
            actor: if target.is_empty() { "DbContext" } else { target }.to_string(),
            method: "save".to_string(),
            kind: FlowStepKind::DbWrite,
            description: format!("DB write: {}", target),
            condition: None,
            children: Vec::new(),
        });
    }

    if text.contains(".Remove(") || text.contains(".RemoveRange(") {
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

    // EF Core DB reads
    if text.contains(".Find(")
        || text.contains(".FindAsync(")
        || text.contains(".FirstOrDefault(")
        || text.contains(".FirstOrDefaultAsync(")
        || text.contains(".SingleOrDefault(")
        || text.contains(".Where(")
        || text.contains(".ToList(")
        || text.contains(".ToListAsync(")
    {
        let target = text.split('.').next().unwrap_or("").trim();
        return Some(FlowStep {
            actor: target.to_string(),
            method: "query".to_string(),
            kind: FlowStepKind::DbRead,
            description: format!("DB read: {}", target),
            condition: None,
            children: Vec::new(),
        });
    }

    // MediatR / Events
    if text.contains(".Publish(")
        || text.contains(".PublishAsync(")
        || text.contains(".Send(")
        || text.contains(".SendAsync(")
    {
        return Some(FlowStep {
            actor: "Mediator".to_string(),
            method: "publish".to_string(),
            kind: FlowStepKind::EventPublish,
            description: "Publish event".to_string(),
            condition: None,
            children: Vec::new(),
        });
    }

    // Service calls: _service.Method() or service.Method()
    if text.contains('.') && text.contains('(') {
        let parts: Vec<&str> = text.splitn(2, '.').collect();
        if parts.len() == 2 {
            let target = parts[0].trim().trim_start_matches("await ");
            let method = parts[1].split('(').next().unwrap_or("").trim();

            if !target.is_empty()
                && (target.starts_with('_')
                    || target.chars().next().is_some_and(|c| c.is_lowercase()))
                && !is_excluded_cs_target(target)
                && !is_excluded_cs_method(method)
            {
                return Some(FlowStep {
                    actor: target.trim_start_matches('_').to_string(),
                    method: method.to_string(),
                    kind: FlowStepKind::ServiceCall,
                    description: format!("Call: {}.{}()", target.trim_start_matches('_'), method),
                    condition: None,
                    children: Vec::new(),
                });
            }
        }
    }

    None
}

fn is_excluded_cs_target(target: &str) -> bool {
    matches!(
        target,
        "this"
            | "base"
            | "Console"
            | "Math"
            | "String"
            | "Convert"
            | "Enum"
            | "Task"
            | "Guid"
            | "DateTime"
            | "response"
            | "logger"
            | "_logger"
            | "Log"
    )
}

fn is_excluded_cs_method(method: &str) -> bool {
    matches!(
        method,
        "ToString"
            | "Equals"
            | "GetHashCode"
            | "GetType"
            | "Select"
            | "Where"
            | "OrderBy"
            | "OrderByDescending"
            | "GroupBy"
            | "Any"
            | "All"
            | "Count"
            | "Sum"
            | "Max"
            | "Min"
            | "ToList"
            | "ToArray"
            | "ToDictionary"
            | "AsNoTracking"
            | "Include"
            | "ThenInclude"
            | "ConfigureAwait"
            | "Log"
            | "LogInformation"
            | "LogWarning"
            | "LogError"
            | "LogDebug"
    )
}

// ==================== Text-based Fallback ====================

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

    fn walk<'a>(node: &'a tree_sitter::Node<'a>, kind: &str, source: &[u8], attr: &str) -> Option<String> {
        if node.kind() == kind
            && let Some(body) = node.child_by_field_name("body") {
                for i in 0..body.named_child_count() {
                    let member = body.named_child(i).unwrap();
                    if member.kind() == "method_declaration" {
                        let attrs = collect_attributes(&member, source);
                        if attrs.iter().any(|a| a.name == attr)
                            && let Some(mb) = member.child_by_field_name("body") {
                                return Some(node_text(&mb, source));
                            }
                    }
                }
            }
        for i in 0..node.named_child_count() {
            let child = node.named_child(i).unwrap();
            if let Some(r) = walk(&child, kind, source, attr) {
                return Some(r);
            }
        }
        None
    }

    walk(root, "class_declaration", source, http_attr)
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
