use crate::context::AnalysisContext;
use crate::extractors::common::*;
use pecto_core::model::*;

const MAX_DEPTH: usize = 4;

/// Extract request flows for TypeScript endpoints.
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

            // Security step
            if let Some(sec) = &endpoint.security
                && sec.authentication.is_some()
            {
                steps.push(FlowStep {
                    actor: cap.name.clone(),
                    method: "guard".to_string(),
                    kind: FlowStepKind::SecurityGuard,
                    description: format!(
                        "Auth: {}",
                        sec.roles
                            .first()
                            .map(|r| r.as_str())
                            .unwrap_or("required")
                    ),
                    condition: None,
                    children: Vec::new(),
                });
            }

            // Trace method body via AST
            let method_steps =
                if let Some(method_body) = find_endpoint_method_body(&root, source, endpoint) {
                    trace_method_body(&method_body, source, ctx, 0)
                } else {
                    // Fallback: text-based scanning
                    let method_source = find_endpoint_source(&file.source, endpoint)
                        .unwrap_or_else(|| file.source.clone());
                    let mut fallback_steps = Vec::new();
                    trace_source_text(&method_source, &mut fallback_steps);
                    fallback_steps
                };
            steps.extend(method_steps);

            // Return step
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

/// Find the specific method body node for an endpoint using decorators.
fn find_endpoint_method_body<'a>(
    root: &'a tree_sitter::Node<'a>,
    source: &[u8],
    endpoint: &Endpoint,
) -> Option<tree_sitter::Node<'a>> {
    let method_lower = format!("{:?}", endpoint.method).to_lowercase();

    for i in 0..root.named_child_count() {
        let node = root.named_child(i).unwrap();

        // NestJS class: look at class body children
        if node.kind() == "class_declaration" || node.kind() == "export_statement" {
            // May be wrapped in export_statement
            let class_node = if node.kind() == "export_statement" {
                let mut found = None;
                for k in 0..node.named_child_count() {
                    let c = node.named_child(k).unwrap();
                    if c.kind() == "class_declaration" {
                        found = Some(c);
                        break;
                    }
                }
                match found {
                    Some(c) => c,
                    None => continue,
                }
            } else {
                node
            };

            if let Some(body) = class_node.child_by_field_name("body") {
                // Track decorators seen before each method
                let mut pending_decorators: Vec<String> = Vec::new();

                // Use ALL children (named + unnamed) to catch decorators
                for j in 0..body.child_count() {
                    let member = body.child(j).unwrap();

                    if member.kind() == "decorator" {
                        let dec_text = node_text(&member, source).to_lowercase();
                        pending_decorators.push(dec_text);
                        continue;
                    }

                    if member.kind() == "method_definition" {
                        // Also check decorators as direct children of method_definition
                        let mut method_decorators = Vec::new();
                        for k in 0..member.child_count() {
                            let child = member.child(k).unwrap();
                            if child.kind() == "decorator" {
                                method_decorators
                                    .push(node_text(&child, source).to_lowercase());
                            }
                        }

                        let all_decorators: Vec<&String> = pending_decorators
                            .iter()
                            .chain(method_decorators.iter())
                            .collect();

                        for dec_text in &all_decorators {
                            let has_method = dec_text.contains(&format!("@{}(", method_lower))
                                || dec_text.contains(&format!("@{}", method_lower));

                            if !has_method {
                                continue;
                            }

                            // Bare decorator: @Get() or @Get with no args
                            let dec_is_bare = dec_text.contains("()")
                                || dec_text.trim().ends_with(&format!("@{}", method_lower));

                            // Does endpoint have path params like {id}?
                            let has_path_params = endpoint.path.contains('{');

                            let path_ok = if has_path_params {
                                // Endpoint like /cats/{id} → decorator must have matching param
                                // Find the {param} segment and check for both {param} and :param
                                endpoint.path.rsplit('/').any(|seg| {
                                    if seg.starts_with('{') && seg.ends_with('}') {
                                        let colon = format!(":{}", &seg[1..seg.len()-1]);
                                        dec_text.contains(seg) || dec_text.contains(&colon)
                                    } else {
                                        false
                                    }
                                })
                            } else {
                                // Endpoint like /cats → decorator should be bare @Get()
                                dec_is_bare
                            };

                            if path_ok {
                                return member.child_by_field_name("body");
                            }
                        }

                        pending_decorators.clear();
                    } else if member.kind() != "comment" {
                        pending_decorators.clear();
                    }
                }
            }
        }

        // Next.js: export function GET
        if node.kind() == "export_statement" {
            for j in 0..node.named_child_count() {
                let child = node.named_child(j).unwrap();
                if child.kind() == "function_declaration" || child.kind() == "lexical_declaration" {
                    let name = child
                        .child_by_field_name("name")
                        .map(|n| node_text(&n, source))
                        .unwrap_or_default();
                    let method_upper = format!("{:?}", endpoint.method);
                    if name == method_upper {
                        return child.child_by_field_name("body");
                    }
                }
            }
        }
    }
    None
}

/// Trace a method body node, resolving internal method calls.
fn trace_method_body(
    body: &tree_sitter::Node,
    source: &[u8],
    _ctx: &AnalysisContext,
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

/// Walk up the tree to find the enclosing class body.
fn find_enclosing_class_body<'a>(
    node: &'a tree_sitter::Node<'a>,
) -> Option<tree_sitter::Node<'a>> {
    let mut current = node.parent();
    while let Some(n) = current {
        if n.kind() == "class_declaration" || n.kind() == "class" {
            return n.child_by_field_name("body");
        }
        current = n.parent();
    }
    None
}

/// Find a method by name within a class body.
fn find_method_in_class<'a>(
    class_body: &'a tree_sitter::Node<'a>,
    method_name: &str,
    source: &[u8],
) -> Option<tree_sitter::Node<'a>> {
    for i in 0..class_body.named_child_count() {
        let member = class_body.named_child(i).unwrap();
        if member.kind() == "method_definition"
            && let Some(name_node) = member.child_by_field_name("name")
                && node_text(&name_node, source) == method_name {
                    return member.child_by_field_name("body");
                }
    }
    None
}

/// Recursively walk AST nodes, extracting flow steps.
fn trace_node_recursive(
    node: &tree_sitter::Node,
    source: &[u8],
    depth: usize,
    steps: &mut Vec<FlowStep>,
    class_body: Option<&tree_sitter::Node>,
) {
    match node.kind() {
        "call_expression" => {
            let text = node_text(node, source);

            // Check for this.method() or this.service.method() calls
            if let Some(func) = node.child_by_field_name("function")
                && func.kind() == "member_expression"
                    && let Some(obj) = func.child_by_field_name("object") {
                        let obj_text = node_text(&obj, source);

                        // this.method() → try to resolve as internal class method
                        if obj_text == "this"
                            && let Some(prop) = func.child_by_field_name("property") {
                                let method_name = node_text(&prop, source);
                                if let Some(cb) = class_body
                                    && depth < MAX_DEPTH
                                        && let Some(target_body) =
                                            find_method_in_class(cb, &method_name, source)
                                        {
                                            trace_node_recursive(
                                                &target_body,
                                                source,
                                                depth + 1,
                                                steps,
                                                Some(cb),
                                            );
                                            return;
                                        }
                            }

                        // this.service.method() → service call
                        if obj.kind() == "member_expression"
                            && let Some(inner_obj) = obj.child_by_field_name("object")
                                && node_text(&inner_obj, source) == "this"
                                    && let Some(svc) = obj.child_by_field_name("property")
                                        && let Some(method) = func.child_by_field_name("property")
                                        {
                                            let svc_name = node_text(&svc, source);
                                            let method_name = node_text(&method, source);
                                            if !is_excluded_ts_method(&method_name) {
                                                steps.push(FlowStep {
                                                    actor: svc_name.clone(),
                                                    method: method_name.clone(),
                                                    kind: FlowStepKind::ServiceCall,
                                                    description: format!(
                                                        "Call: {}.{}()",
                                                        svc_name, method_name
                                                    ),
                                                    condition: None,
                                                    children: Vec::new(),
                                                });
                                                return;
                                            }
                                        }
                    }

            // Classify the call by text patterns
            if let Some(step) = classify_method_call(&text) {
                steps.push(step);
                return;
            }
        }
        "throw_statement" => {
            let text = node_text(node, source);
            let exception = text
                .split("new ")
                .nth(1)
                .and_then(|s| s.split('(').next())
                .unwrap_or("Error")
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
                    &alternative,
                    source,
                    depth + 1,
                    &mut else_children,
                    class_body,
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

    // Recurse into children
    for i in 0..node.child_count() {
        let child = node.child(i).unwrap();
        trace_node_recursive(&child, source, depth, steps, class_body);
    }
}

/// Classify a method call text into a FlowStep.
fn classify_method_call(text: &str) -> Option<FlowStep> {
    // DB write operations
    if text.contains(".save(")
        || text.contains(".insert(")
        || text.contains(".update(")
        || text.contains(".insertOne(")
        || text.contains(".updateOne(")
    {
        let target = text.split('.').next().unwrap_or("").trim();
        return Some(FlowStep {
            actor: target.to_string(),
            method: "save".to_string(),
            kind: FlowStepKind::DbWrite,
            description: format!("DB write: {}", target),
            condition: None,
            children: Vec::new(),
        });
    }

    if text.contains(".create(") && !text.contains("createElement") {
        let target = text.split('.').next().unwrap_or("").trim();
        // Heuristic: .create() on a model/repository is a DB write
        if !target.is_empty()
            && target.chars().next().is_some_and(|c| c.is_lowercase())
            && !matches!(target, "document" | "Object" | "Array")
        {
            return Some(FlowStep {
                actor: target.to_string(),
                method: "create".to_string(),
                kind: FlowStepKind::DbWrite,
                description: format!("DB write: {}.create()", target),
                condition: None,
                children: Vec::new(),
            });
        }
    }

    if text.contains(".delete(") || text.contains(".remove(") || text.contains(".deleteOne(") {
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

    // DB read operations
    if text.contains(".find(")
        || text.contains(".findOne(")
        || text.contains(".findById(")
        || text.contains(".findAll(")
        || text.contains(".findOneBy(")
        || text.contains(".query(")
    {
        let target = text.split('.').next().unwrap_or("").trim();
        return Some(FlowStep {
            actor: target.to_string(),
            method: "find".to_string(),
            kind: FlowStepKind::DbRead,
            description: format!("DB read: {}", target),
            condition: None,
            children: Vec::new(),
        });
    }

    // Event publishing
    if text.contains(".emit(") || text.contains(".publish(") {
        let event = text
            .split(".emit(")
            .nth(1)
            .or_else(|| text.split(".publish(").nth(1))
            .and_then(|s| s.split(')').next())
            .unwrap_or("event");
        return Some(FlowStep {
            actor: "EventBus".to_string(),
            method: "emit".to_string(),
            kind: FlowStepKind::EventPublish,
            description: format!("Emit: {}", event.chars().take(60).collect::<String>()),
            condition: None,
            children: Vec::new(),
        });
    }

    // Service calls: object.method() where object is a lowercase field
    if text.contains('.') && text.contains('(') {
        let parts: Vec<&str> = text.splitn(2, '.').collect();
        if parts.len() == 2 {
            let target = parts[0].trim();
            let method = parts[1].split('(').next().unwrap_or("").trim();

            // this.service.method() → extract service.method
            if target == "this" && parts[1].contains('.') {
                let inner_parts: Vec<&str> = parts[1].splitn(2, '.').collect();
                if inner_parts.len() == 2 {
                    let service = inner_parts[0].trim();
                    let svc_method = inner_parts[1].split('(').next().unwrap_or("").trim();
                    if !service.is_empty()
                        && service.chars().next().is_some_and(|c| c.is_lowercase())
                        && !is_excluded_ts_method(svc_method)
                    {
                        return Some(FlowStep {
                            actor: service.to_string(),
                            method: svc_method.to_string(),
                            kind: FlowStepKind::ServiceCall,
                            description: format!("Call: {}.{}()", service, svc_method),
                            condition: None,
                            children: Vec::new(),
                        });
                    }
                }
            }

            if !target.is_empty()
                && target.chars().next().is_some_and(|c| c.is_lowercase())
                && !is_excluded_ts_target(target)
                && !is_excluded_ts_method(method)
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

fn is_excluded_ts_target(target: &str) -> bool {
    matches!(
        target,
        "this"
            | "console"
            | "Math"
            | "JSON"
            | "Object"
            | "Array"
            | "Promise"
            | "Buffer"
            | "Date"
            | "RegExp"
            | "Error"
            | "process"
            | "window"
            | "document"
            | "response"
            | "res"
            | "req"
            | "request"
    )
}

fn is_excluded_ts_method(method: &str) -> bool {
    matches!(
        method,
        "toString"
            | "valueOf"
            | "map"
            | "filter"
            | "reduce"
            | "forEach"
            | "then"
            | "catch"
            | "finally"
            | "push"
            | "pop"
            | "shift"
            | "unshift"
            | "join"
            | "split"
            | "slice"
            | "splice"
            | "concat"
            | "includes"
            | "indexOf"
            | "keys"
            | "values"
            | "entries"
            | "log"
            | "warn"
            | "error"
            | "info"
            | "debug"
            | "status"
            | "json"
            | "send"
            | "pipe"
            | "subscribe"
            | "toPromise"
            | "bind"
            | "apply"
            | "call"
    )
}

// ==================== Text-based Fallback ====================

/// Find endpoint source text (for non-NestJS: Express, Next.js).
fn find_endpoint_source(source: &str, endpoint: &Endpoint) -> Option<String> {
    let method_lower = format!("{:?}", endpoint.method).to_lowercase();
    let method_upper = format!("{:?}", endpoint.method);

    let lines: Vec<&str> = source.lines().collect();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        // Express: router.get('/path', ...) or app.get('/path', ...)
        if trimmed.contains(&format!(".{}(", method_lower)) && trimmed.contains(&endpoint.path) {
            return extract_brace_block(&lines, i);
        }

        // Next.js: export function GET
        if trimmed.contains(&format!("function {}", method_upper))
            || trimmed.contains(&format!("const {} ", method_upper))
        {
            return extract_brace_block(&lines, i);
        }
    }
    None
}

fn extract_brace_block(lines: &[&str], start: usize) -> Option<String> {
    let mut depth = 0;
    let mut started = false;
    let mut result = Vec::new();

    for line in &lines[start..] {
        for ch in line.chars() {
            if ch == '{' {
                depth += 1;
                started = true;
            } else if ch == '}' {
                depth -= 1;
            }
        }
        if started {
            result.push(*line);
        }
        if started && depth == 0 {
            return Some(result.join("\n"));
        }
    }
    None
}

fn trace_source_text(source: &str, steps: &mut Vec<FlowStep>) {
    for line in source.lines() {
        let trimmed = line.trim();

        if trimmed.contains(".save(") || trimmed.contains(".create(") {
            steps.push(FlowStep {
                actor: "Repository".to_string(),
                method: "save".to_string(),
                kind: FlowStepKind::DbWrite,
                description: "DB write".to_string(),
                condition: None,
                children: Vec::new(),
            });
        } else if trimmed.contains(".find(") || trimmed.contains(".findOne(") {
            steps.push(FlowStep {
                actor: "Repository".to_string(),
                method: "find".to_string(),
                kind: FlowStepKind::DbRead,
                description: "DB read".to_string(),
                condition: None,
                children: Vec::new(),
            });
        } else if trimmed.contains(".emit(") || trimmed.contains(".publish(") {
            steps.push(FlowStep {
                actor: "EventBus".to_string(),
                method: "emit".to_string(),
                kind: FlowStepKind::EventPublish,
                description: "Emit event".to_string(),
                condition: None,
                children: Vec::new(),
            });
        }
    }
}
