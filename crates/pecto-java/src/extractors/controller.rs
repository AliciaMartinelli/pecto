use super::common::*;
use crate::context::{AnalysisContext, ParsedFile};
use pecto_core::model::*;
use std::collections::BTreeMap;
use tree_sitter::Node;

/// Known HTTP status mappings from Spring's HttpStatus enum.
const STATUS_MAPPINGS: &[(&str, u16, &str)] = &[
    ("NOT_FOUND", 404, "not-found"),
    ("BAD_REQUEST", 400, "bad-request"),
    ("UNAUTHORIZED", 401, "unauthorized"),
    ("FORBIDDEN", 403, "forbidden"),
    ("CONFLICT", 409, "conflict"),
    ("GONE", 410, "gone"),
    ("UNPROCESSABLE_ENTITY", 422, "unprocessable-entity"),
    ("INTERNAL_SERVER_ERROR", 500, "internal-error"),
    ("SERVICE_UNAVAILABLE", 503, "service-unavailable"),
    ("NO_CONTENT", 204, "no-content"),
    ("CREATED", 201, "created"),
    ("OK", 200, "success"),
    ("ACCEPTED", 202, "accepted"),
];

/// Extract a Capability from a parsed file if it contains a Spring controller or JAX-RS resource.
pub fn extract(file: &ParsedFile, ctx: &AnalysisContext) -> Option<Capability> {
    let root = file.tree.root_node();
    let source_bytes = file.source.as_bytes();

    for i in 0..root.named_child_count() {
        let node = root.named_child(i).unwrap();
        // Support both class_declaration and interface_declaration (JAX-RS uses interfaces)
        if node.kind() != "class_declaration" && node.kind() != "interface_declaration" {
            continue;
        }

        let annotations = collect_annotations(&node, source_bytes);

        // Spring: @RestController or @Controller
        let is_spring_controller = annotations
            .iter()
            .any(|a| a.name == "RestController" || a.name == "Controller");

        // JAX-RS: @Path on class/interface
        let is_jaxrs_resource = annotations.iter().any(|a| a.name == "Path");

        if !is_spring_controller && !is_jaxrs_resource {
            continue;
        }

        let class_name = get_class_name(&node, source_bytes);

        let (base_path, capability_name) = if is_jaxrs_resource {
            let path = extract_jaxrs_path(&annotations);
            let name = to_kebab_case(&class_name.replace("Resource", "").replace("Controller", ""));
            (path, name)
        } else {
            let path = extract_request_mapping_path(&annotations);
            let name = to_kebab_case(&class_name.replace("Controller", ""));
            (path, name)
        };

        let mut capability = Capability::new(capability_name, file.path.clone());
        let class_cors = extract_cors(&annotations);
        let class_rate_limit = extract_rate_limit(&annotations);

        // Extract security from JAX-RS annotations
        let class_security = if is_jaxrs_resource {
            extract_jaxrs_security(&annotations)
        } else {
            None
        };

        if is_jaxrs_resource {
            extract_jaxrs_endpoints_from_class(
                &node,
                source_bytes,
                &base_path,
                &mut capability,
                class_security.as_ref(),
            );
        } else {
            extract_endpoints_from_class(
                &node,
                source_bytes,
                &base_path,
                &mut capability,
                ctx,
                class_cors.as_deref(),
                class_rate_limit.as_deref(),
            );
        }

        return Some(capability);
    }

    None
}

fn extract_request_mapping_path(annotations: &[AnnotationInfo]) -> String {
    for ann in annotations {
        if ann.name == "RequestMapping" {
            if let Some(val) = &ann.value {
                return val.clone();
            }
            if let Some(val) = ann.arguments.get("value") {
                return val.clone();
            }
            if let Some(path) = ann.arguments.get("path") {
                return path.clone();
            }
        }
    }
    String::new()
}

fn extract_endpoints_from_class(
    class_node: &Node,
    source: &[u8],
    base_path: &str,
    capability: &mut Capability,
    ctx: &AnalysisContext,
    class_cors: Option<&str>,
    class_rate_limit: Option<&str>,
) {
    let body = match class_node.child_by_field_name("body") {
        Some(b) => b,
        None => return,
    };

    // First pass: collect @ExceptionHandler error behaviors
    let exception_behaviors = extract_exception_handler_behaviors(&body, source);

    // Second pass: extract endpoints and attach exception handler behaviors
    for i in 0..body.named_child_count() {
        let member = body.named_child(i).unwrap();
        if member.kind() == "method_declaration" {
            let annotations = collect_annotations(&member, source);

            if let Some(mut endpoint) = extract_endpoint_from_method(
                &member,
                source,
                &annotations,
                base_path,
                ctx,
                class_cors,
                class_rate_limit,
            ) {
                // Add exception handler behaviors to every endpoint
                for b in &exception_behaviors {
                    if !endpoint.behaviors.iter().any(|eb| eb.name == b.name) {
                        endpoint.behaviors.push(b.clone());
                    }
                }
                capability.endpoints.push(endpoint);
            }
        }
    }
}

fn extract_endpoint_from_method(
    method_node: &Node,
    source: &[u8],
    annotations: &[AnnotationInfo],
    base_path: &str,
    ctx: &AnalysisContext,
    class_cors: Option<&str>,
    class_rate_limit: Option<&str>,
) -> Option<Endpoint> {
    let (http_method, method_path) = extract_http_method_and_path(annotations)?;

    let full_path = if base_path.is_empty() {
        method_path
    } else {
        format!("{}{}", base_path.trim_end_matches('/'), method_path)
    };

    let input = extract_method_input(method_node, source, ctx);
    let validation = extract_validation_rules(method_node, source, ctx);

    let return_type = method_node
        .child_by_field_name("type")
        .map(|t| node_text(&t, source));

    let security = extract_security_config(annotations, class_cors, class_rate_limit);

    // Determine success status: @ResponseStatus annotation overrides default
    let success_status = extract_response_status_annotation(annotations)
        .unwrap_or_else(|| default_success_status(&http_method));

    let mut behaviors = vec![Behavior {
        name: "success".to_string(),
        condition: None,
        returns: ResponseSpec {
            status: success_status,
            body: return_type.map(|t| TypeRef {
                name: clean_generic_type(&t),
                fields: BTreeMap::new(),
            }),
        },
        side_effects: Vec::new(),
    }];

    // Extract error behaviors from method body
    if let Some(body) = method_node.child_by_field_name("body") {
        extract_error_behaviors(&body, source, &mut behaviors);
    }

    Some(Endpoint {
        method: http_method,
        path: full_path,
        input,
        validation,
        behaviors,
        security,
    })
}

pub(crate) fn extract_http_method_and_path(annotations: &[AnnotationInfo]) -> Option<(HttpMethod, String)> {
    for ann in annotations {
        let (method, path) = match ann.name.as_str() {
            "GetMapping" => (HttpMethod::Get, extract_path_from_annotation(ann)),
            "PostMapping" => (HttpMethod::Post, extract_path_from_annotation(ann)),
            "PutMapping" => (HttpMethod::Put, extract_path_from_annotation(ann)),
            "DeleteMapping" => (HttpMethod::Delete, extract_path_from_annotation(ann)),
            "PatchMapping" => (HttpMethod::Patch, extract_path_from_annotation(ann)),
            "RequestMapping" => {
                let m = ann
                    .arguments
                    .get("method")
                    .map(|m| match m.as_str() {
                        s if s.contains("GET") => HttpMethod::Get,
                        s if s.contains("POST") => HttpMethod::Post,
                        s if s.contains("PUT") => HttpMethod::Put,
                        s if s.contains("DELETE") => HttpMethod::Delete,
                        s if s.contains("PATCH") => HttpMethod::Patch,
                        _ => HttpMethod::Get,
                    })
                    .unwrap_or(HttpMethod::Get);
                (m, extract_path_from_annotation(ann))
            }
            _ => continue,
        };
        return Some((method, path));
    }
    None
}

pub(crate) fn extract_path_from_annotation(ann: &AnnotationInfo) -> String {
    if let Some(val) = &ann.value {
        return val.clone();
    }
    if let Some(val) = ann.arguments.get("value") {
        return val.clone();
    }
    if let Some(path) = ann.arguments.get("path") {
        return path.clone();
    }
    String::new()
}

fn extract_method_input(
    method_node: &Node,
    source: &[u8],
    ctx: &AnalysisContext,
) -> Option<EndpointInput> {
    let params_node = method_node.child_by_field_name("parameters")?;
    let mut body = None;
    let mut path_params = Vec::new();
    let mut query_params = Vec::new();

    for i in 0..params_node.named_child_count() {
        let param = params_node.named_child(i).unwrap();
        if param.kind() != "formal_parameter" {
            continue;
        }

        let annotations = collect_annotations(&param, source);
        let param_type = param
            .child_by_field_name("type")
            .map(|t| node_text(&t, source))
            .unwrap_or_default();
        let param_name = param
            .child_by_field_name("name")
            .map(|n| node_text(&n, source))
            .unwrap_or_default();

        for ann in &annotations {
            match ann.name.as_str() {
                "RequestBody" => {
                    let type_name = clean_generic_type(&param_type);
                    let fields = resolve_type_fields(&type_name, ctx);
                    body = Some(TypeRef {
                        name: type_name,
                        fields,
                    });
                }
                "PathVariable" => {
                    path_params.push(Param {
                        name: ann.value.clone().unwrap_or_else(|| param_name.clone()),
                        param_type: param_type.clone(),
                        required: true,
                    });
                }
                "RequestParam" => {
                    let required = ann
                        .arguments
                        .get("required")
                        .map(|r| r != "false")
                        .unwrap_or(true);
                    query_params.push(Param {
                        name: ann.value.clone().unwrap_or_else(|| param_name.clone()),
                        param_type: param_type.clone(),
                        required,
                    });
                }
                _ => {}
            }
        }
    }

    if body.is_none() && path_params.is_empty() && query_params.is_empty() {
        return None;
    }

    Some(EndpointInput {
        body,
        path_params,
        query_params,
    })
}

fn extract_validation_rules(
    method_node: &Node,
    source: &[u8],
    ctx: &AnalysisContext,
) -> Vec<ValidationRule> {
    let mut rules = Vec::new();
    let params_node = match method_node.child_by_field_name("parameters") {
        Some(p) => p,
        None => return rules,
    };

    for i in 0..params_node.named_child_count() {
        let param = params_node.named_child(i).unwrap();
        if param.kind() != "formal_parameter" {
            continue;
        }

        let annotations = collect_annotations(&param, source);
        let has_valid = annotations
            .iter()
            .any(|a| a.name == "Valid" || a.name == "Validated");

        if has_valid {
            let param_type = param
                .child_by_field_name("type")
                .map(|t| node_text(&t, source))
                .unwrap_or_default();
            let type_name = clean_generic_type(&param_type);

            // Try to resolve field-level validation from the DTO class
            let field_rules = resolve_validation_rules(&type_name, ctx);
            if field_rules.is_empty() {
                // Fallback: just record @Valid on the parameter
                let param_name = param
                    .child_by_field_name("name")
                    .map(|n| node_text(&n, source))
                    .unwrap_or_default();
                rules.push(ValidationRule {
                    field: param_name,
                    constraints: vec!["@Valid".to_string()],
                });
            } else {
                rules.extend(field_rules);
            }
        }
    }

    rules
}

fn extract_security_config(
    annotations: &[AnnotationInfo],
    class_cors: Option<&str>,
    class_rate_limit: Option<&str>,
) -> Option<SecurityConfig> {
    let mut auth = None;
    let mut roles = Vec::new();

    for ann in annotations {
        match ann.name.as_str() {
            "PreAuthorize" => {
                if let Some(val) = &ann.value {
                    if val.contains("hasRole") || val.contains("hasAuthority") {
                        roles.push(val.clone());
                    }
                    auth = Some("required".to_string());
                }
            }
            "Secured" => {
                if let Some(val) = &ann.value {
                    roles.push(val.clone());
                }
                auth = Some("required".to_string());
            }
            _ => {}
        }
    }

    // Method-level CORS/rate-limit override class-level
    let cors = extract_cors(annotations).or_else(|| class_cors.map(String::from));
    let rate_limit = extract_rate_limit(annotations).or_else(|| class_rate_limit.map(String::from));

    if auth.is_none() && roles.is_empty() && cors.is_none() && rate_limit.is_none() {
        return None;
    }

    Some(SecurityConfig {
        authentication: auth,
        roles,
        rate_limit,
        cors,
    })
}

/// Extract @CrossOrigin annotation details.
fn extract_cors(annotations: &[AnnotationInfo]) -> Option<String> {
    let ann = annotations.iter().find(|a| a.name == "CrossOrigin")?;

    let origins = ann
        .arguments
        .get("origins")
        .or(ann.value.as_ref())
        .cloned()
        .unwrap_or_else(|| "*".to_string());

    Some(format!("origins: {}", origins))
}

/// Extract rate limiting annotation (@RateLimiter from Resilience4j, @RateLimit).
fn extract_rate_limit(annotations: &[AnnotationInfo]) -> Option<String> {
    for ann in annotations {
        match ann.name.as_str() {
            "RateLimiter" => {
                let name = ann
                    .arguments
                    .get("name")
                    .cloned()
                    .unwrap_or_else(|| "default".to_string());
                return Some(format!("rate-limiter: {}", name));
            }
            "RateLimit" | "RateLimited" => {
                return Some("rate-limited".to_string());
            }
            _ => {}
        }
    }
    None
}

/// Resolve field names and types from a DTO class found in the AnalysisContext.
fn resolve_type_fields(type_name: &str, ctx: &AnalysisContext) -> BTreeMap<String, String> {
    let mut fields = BTreeMap::new();
    let Some(file) = ctx.find_class_by_name(type_name) else {
        return fields;
    };

    let root = file.tree.root_node();
    let source = file.source.as_bytes();

    for i in 0..root.named_child_count() {
        let node = root.named_child(i).unwrap();
        if node.kind() == "class_declaration"
            && get_class_name(&node, source) == type_name
            && let Some(body) = node.child_by_field_name("body")
        {
            for j in 0..body.named_child_count() {
                let member = body.named_child(j).unwrap();
                if member.kind() == "field_declaration" {
                    let field_type = member
                        .child_by_field_name("type")
                        .map(|t| node_text(&t, source))
                        .unwrap_or_default();
                    let field_name = extract_declarator_name(&member, source);
                    if !field_name.is_empty() {
                        fields.insert(field_name, field_type);
                    }
                }
            }
        }
    }

    fields
}

/// Resolve field-level validation rules from a DTO class.
fn resolve_validation_rules(type_name: &str, ctx: &AnalysisContext) -> Vec<ValidationRule> {
    let mut rules = Vec::new();
    let Some(file) = ctx.find_class_by_name(type_name) else {
        return rules;
    };

    let root = file.tree.root_node();
    let source = file.source.as_bytes();

    for i in 0..root.named_child_count() {
        let node = root.named_child(i).unwrap();
        if node.kind() == "class_declaration"
            && get_class_name(&node, source) == type_name
            && let Some(body) = node.child_by_field_name("body")
        {
            for j in 0..body.named_child_count() {
                let member = body.named_child(j).unwrap();
                if member.kind() != "field_declaration" {
                    continue;
                }

                let annotations = collect_annotations(&member, source);
                let constraints = extract_validation_constraints(&annotations);
                if constraints.is_empty() {
                    continue;
                }

                let field_name = extract_declarator_name(&member, source);
                if !field_name.is_empty() {
                    rules.push(ValidationRule {
                        field: field_name,
                        constraints,
                    });
                }
            }
        }
    }

    rules
}

/// Extract validation constraint strings from field annotations.
fn extract_validation_constraints(annotations: &[AnnotationInfo]) -> Vec<String> {
    let mut constraints = Vec::new();
    for ann in annotations {
        match ann.name.as_str() {
            "NotNull" => constraints.push("@NotNull".to_string()),
            "NotBlank" => constraints.push("@NotBlank".to_string()),
            "NotEmpty" => constraints.push("@NotEmpty".to_string()),
            "Email" => constraints.push("@Email".to_string()),
            "Size" => {
                let mut parts = Vec::new();
                if let Some(v) = ann.arguments.get("min") {
                    parts.push(format!("min={}", v));
                }
                if let Some(v) = ann.arguments.get("max") {
                    parts.push(format!("max={}", v));
                }
                constraints.push(format!("@Size({})", parts.join(", ")));
            }
            "Min" => {
                let val = ann.value.as_deref().unwrap_or("?");
                constraints.push(format!("@Min({})", val));
            }
            "Max" => {
                let val = ann.value.as_deref().unwrap_or("?");
                constraints.push(format!("@Max({})", val));
            }
            "Pattern" => {
                let regexp = ann
                    .arguments
                    .get("regexp")
                    .map(|r| r.as_str())
                    .unwrap_or("?");
                constraints.push(format!("@Pattern(regexp={})", regexp));
            }
            "Positive" => constraints.push("@Positive".to_string()),
            "Negative" => constraints.push("@Negative".to_string()),
            "Past" => constraints.push("@Past".to_string()),
            "Future" => constraints.push("@Future".to_string()),
            _ => {}
        }
    }
    constraints
}

/// Extract the variable name from a field_declaration's variable_declarator.
fn extract_declarator_name(field_decl: &Node, source: &[u8]) -> String {
    for i in 0..field_decl.named_child_count() {
        let child = field_decl.named_child(i).unwrap();
        if child.kind() == "variable_declarator"
            && let Some(name) = child.child_by_field_name("name")
        {
            return node_text(&name, source);
        }
    }
    String::new()
}

fn default_success_status(method: &HttpMethod) -> u16 {
    match method {
        HttpMethod::Post => 201,
        HttpMethod::Delete => 204,
        _ => 200,
    }
}

/// Extract status code from @ResponseStatus annotation on a method.
fn extract_response_status_annotation(annotations: &[AnnotationInfo]) -> Option<u16> {
    for ann in annotations {
        if ann.name == "ResponseStatus" {
            // @ResponseStatus(HttpStatus.CREATED) or @ResponseStatus(code = HttpStatus.CREATED)
            let status_text = ann
                .arguments
                .get("code")
                .or_else(|| ann.arguments.get("value"))
                .or(ann.value.as_ref())?;
            return resolve_http_status(status_text);
        }
    }
    None
}

/// Resolve a Spring HttpStatus reference to a numeric status code.
fn resolve_http_status(text: &str) -> Option<u16> {
    for &(name, code, _) in STATUS_MAPPINGS {
        if text.contains(name) {
            return Some(code);
        }
    }
    // Try parsing as a number directly
    text.parse::<u16>().ok()
}

/// Extract error behaviors from throw statements and ResponseEntity patterns in a method body.
fn extract_error_behaviors(body: &Node, source: &[u8], behaviors: &mut Vec<Behavior>) {
    extract_error_behaviors_recursive(body, source, behaviors);
}

fn extract_error_behaviors_recursive(node: &Node, source: &[u8], behaviors: &mut Vec<Behavior>) {
    match node.kind() {
        "throw_statement" => {
            if let Some(behavior) = extract_throw_behavior(node, source)
                && !behaviors.iter().any(|b| b.name == behavior.name)
            {
                behaviors.push(behavior);
            }
        }
        "return_statement" => {
            if let Some(behavior) = extract_response_entity_behavior(node, source)
                && behavior.returns.status >= 400
                && !behaviors.iter().any(|b| b.name == behavior.name)
            {
                behaviors.push(behavior);
            }
        }
        // Catch `new ResponseStatusException(...)` in lambdas/expressions (not just throw)
        "object_creation_expression" => {
            let text = node_text(node, source);
            if text.contains("ResponseStatusException") {
                for &(name, code, behavior_name) in STATUS_MAPPINGS {
                    if code >= 400 && text.contains(name) {
                        if !behaviors.iter().any(|b| b.name == behavior_name) {
                            behaviors.push(Behavior {
                                name: behavior_name.to_string(),
                                condition: None,
                                returns: ResponseSpec {
                                    status: code,
                                    body: None,
                                },
                                side_effects: Vec::new(),
                            });
                        }
                        break;
                    }
                }
            }
        }
        _ => {}
    }

    for i in 0..node.child_count() {
        let child = node.child(i).unwrap();
        extract_error_behaviors_recursive(&child, source, behaviors);
    }
}

/// Extract a behavior from `throw new ResponseStatusException(HttpStatus.NOT_FOUND, ...)`.
fn extract_throw_behavior(throw_node: &Node, source: &[u8]) -> Option<Behavior> {
    let text = node_text(throw_node, source);

    // Match: throw new ResponseStatusException(HttpStatus.XXX, "message")
    if text.contains("ResponseStatusException") {
        for &(name, code, behavior_name) in STATUS_MAPPINGS {
            if text.contains(name) {
                return Some(Behavior {
                    name: behavior_name.to_string(),
                    condition: None,
                    returns: ResponseSpec {
                        status: code,
                        body: None,
                    },
                    side_effects: Vec::new(),
                });
            }
        }
    }

    // Match: throw new XxxNotFoundException(...)
    let exception_name = extract_exception_class_name(&text)?;
    let (status, behavior_name) = infer_status_from_exception_name(&exception_name);
    Some(Behavior {
        name: behavior_name,
        condition: None,
        returns: ResponseSpec { status, body: None },
        side_effects: Vec::new(),
    })
}

/// Extract the exception class name from a throw statement text.
fn extract_exception_class_name(text: &str) -> Option<String> {
    // "throw new SomethingNotFoundException(...)"
    let after_new = text.split("new ").nth(1)?;
    let class_name = after_new.split('(').next()?.trim();
    if class_name.is_empty() {
        return None;
    }
    Some(class_name.to_string())
}

/// Infer HTTP status from exception class name patterns.
fn infer_status_from_exception_name(name: &str) -> (u16, String) {
    let lower = name.to_lowercase();
    if lower.contains("notfound") || lower.contains("not_found") {
        (404, "not-found".to_string())
    } else if lower.contains("unauthorized") || lower.contains("authentication") {
        (401, "unauthorized".to_string())
    } else if lower.contains("forbidden") || lower.contains("accessdenied") {
        (403, "forbidden".to_string())
    } else if lower.contains("badrequest") || lower.contains("invalid") {
        (400, "bad-request".to_string())
    } else if lower.contains("conflict") || lower.contains("duplicate") {
        (409, "conflict".to_string())
    } else {
        (500, "error".to_string())
    }
}

/// Extract a behavior from `return ResponseEntity.notFound().build()` etc.
fn extract_response_entity_behavior(return_node: &Node, source: &[u8]) -> Option<Behavior> {
    let text = node_text(return_node, source);

    if !text.contains("ResponseEntity") {
        return None;
    }

    // ResponseEntity.notFound().build()
    if text.contains(".notFound()") {
        return Some(Behavior {
            name: "not-found".to_string(),
            condition: None,
            returns: ResponseSpec {
                status: 404,
                body: None,
            },
            side_effects: Vec::new(),
        });
    }

    // ResponseEntity.badRequest().body(...)
    if text.contains(".badRequest()") {
        return Some(Behavior {
            name: "bad-request".to_string(),
            condition: None,
            returns: ResponseSpec {
                status: 400,
                body: None,
            },
            side_effects: Vec::new(),
        });
    }

    // ResponseEntity.status(HttpStatus.XXX)
    if text.contains(".status(") {
        for &(name, code, behavior_name) in STATUS_MAPPINGS {
            if text.contains(name) {
                return Some(Behavior {
                    name: behavior_name.to_string(),
                    condition: None,
                    returns: ResponseSpec {
                        status: code,
                        body: None,
                    },
                    side_effects: Vec::new(),
                });
            }
        }
    }

    None
}

/// Extract error behaviors from @ExceptionHandler methods in the controller.
fn extract_exception_handler_behaviors(class_body: &Node, source: &[u8]) -> Vec<Behavior> {
    let mut behaviors = Vec::new();

    for i in 0..class_body.named_child_count() {
        let member = class_body.named_child(i).unwrap();
        if member.kind() != "method_declaration" {
            continue;
        }

        let annotations = collect_annotations(&member, source);
        let is_exception_handler = annotations.iter().any(|a| a.name == "ExceptionHandler");
        if !is_exception_handler {
            continue;
        }

        // Get the status from @ResponseStatus on the handler method
        let status = extract_response_status_annotation(&annotations);

        // Get the exception type from @ExceptionHandler(XxxException.class)
        let handler_ann = annotations.iter().find(|a| a.name == "ExceptionHandler");
        let exception_type = handler_ann.and_then(|a| {
            a.value
                .as_ref()
                .map(|v| v.trim_end_matches(".class").to_string())
        });

        let (inferred_status, behavior_name) = if let Some(ref exc) = exception_type {
            let (s, n) = infer_status_from_exception_name(exc);
            (status.unwrap_or(s), n)
        } else {
            (status.unwrap_or(500), "error".to_string())
        };

        behaviors.push(Behavior {
            name: behavior_name,
            condition: exception_type.map(|e| format!("throws {}", e)),
            returns: ResponseSpec {
                status: inferred_status,
                body: None,
            },
            side_effects: Vec::new(),
        });
    }

    behaviors
}

// ==================== JAX-RS Support ====================

/// Extract path from JAX-RS @Path annotation.
pub(crate) fn extract_jaxrs_path(annotations: &[AnnotationInfo]) -> String {
    for ann in annotations {
        if ann.name == "Path"
            && let Some(val) = &ann.value
        {
            return val.clone();
        }
    }
    String::new()
}

/// Extract security from JAX-RS annotations (@RolesAllowed, @PermitAll, @DenyAll).
fn extract_jaxrs_security(annotations: &[AnnotationInfo]) -> Option<SecurityConfig> {
    let mut roles = Vec::new();
    let mut auth = None;

    for ann in annotations {
        match ann.name.as_str() {
            "RolesAllowed" => {
                if let Some(val) = &ann.value {
                    roles.push(val.clone());
                }
                auth = Some("required".to_string());
            }
            "DenyAll" => {
                auth = Some("denied".to_string());
            }
            "PermitAll" => {
                // Explicitly public — no auth needed
                return None;
            }
            _ => {}
        }
    }

    if auth.is_none() && roles.is_empty() {
        return None;
    }

    Some(SecurityConfig {
        authentication: auth,
        roles,
        rate_limit: None,
        cors: None,
    })
}

/// Extract endpoints from a JAX-RS resource class/interface.
fn extract_jaxrs_endpoints_from_class(
    class_node: &Node,
    source: &[u8],
    base_path: &str,
    capability: &mut Capability,
    class_security: Option<&SecurityConfig>,
) {
    let body = match class_node.child_by_field_name("body") {
        Some(b) => b,
        None => return,
    };

    for i in 0..body.named_child_count() {
        let member = body.named_child(i).unwrap();
        if member.kind() != "method_declaration" {
            continue;
        }

        let annotations = collect_annotations(&member, source);

        if let Some(endpoint) =
            extract_jaxrs_endpoint(&member, source, &annotations, base_path, class_security)
        {
            capability.endpoints.push(endpoint);
        }
    }
}

fn extract_jaxrs_endpoint(
    method_node: &Node,
    source: &[u8],
    annotations: &[AnnotationInfo],
    base_path: &str,
    class_security: Option<&SecurityConfig>,
) -> Option<Endpoint> {
    // Determine HTTP method from @GET, @POST, @PUT, @DELETE, @PATCH
    let http_method = extract_jaxrs_http_method(annotations)?;

    // Combine base path + method path
    let method_path = extract_jaxrs_path(annotations);
    let full_path = if base_path.is_empty() {
        method_path
    } else if method_path.is_empty() {
        base_path.to_string()
    } else {
        format!(
            "{}/{}",
            base_path.trim_end_matches('/'),
            method_path.trim_start_matches('/')
        )
    };

    // Extract parameters
    let input = extract_jaxrs_input(method_node, source);

    // Return type
    let return_type = method_node
        .child_by_field_name("type")
        .map(|t| node_text(&t, source));

    // @PermitAll explicitly means no auth — don't fall back to class-level
    let has_permit_all = annotations.iter().any(|a| a.name == "PermitAll");
    let security = if has_permit_all {
        None
    } else {
        extract_jaxrs_security(annotations).or_else(|| class_security.cloned())
    };

    let behaviors = vec![Behavior {
        name: "success".to_string(),
        condition: None,
        returns: ResponseSpec {
            status: default_success_status(&http_method),
            body: return_type
                .filter(|t| t != "void" && t != "Response")
                .map(|t| TypeRef {
                    name: clean_generic_type(&t),
                    fields: BTreeMap::new(),
                }),
        },
        side_effects: Vec::new(),
    }];

    Some(Endpoint {
        method: http_method,
        path: full_path,
        input,
        validation: Vec::new(),
        behaviors,
        security,
    })
}

pub(crate) fn extract_jaxrs_http_method(annotations: &[AnnotationInfo]) -> Option<HttpMethod> {
    for ann in annotations {
        match ann.name.as_str() {
            "GET" => return Some(HttpMethod::Get),
            "POST" => return Some(HttpMethod::Post),
            "PUT" => return Some(HttpMethod::Put),
            "DELETE" => return Some(HttpMethod::Delete),
            "PATCH" => return Some(HttpMethod::Patch),
            _ => {}
        }
    }
    None
}

fn extract_jaxrs_input(method_node: &Node, source: &[u8]) -> Option<EndpointInput> {
    let params_node = method_node.child_by_field_name("parameters")?;
    let mut body = None;
    let mut path_params = Vec::new();
    let mut query_params = Vec::new();

    for i in 0..params_node.named_child_count() {
        let param = params_node.named_child(i).unwrap();
        if param.kind() != "formal_parameter" {
            continue;
        }

        let annotations = collect_annotations(&param, source);
        let param_type = param
            .child_by_field_name("type")
            .map(|t| node_text(&t, source))
            .unwrap_or_default();
        let param_name = param
            .child_by_field_name("name")
            .map(|n| node_text(&n, source))
            .unwrap_or_default();

        let mut matched = false;
        for ann in &annotations {
            match ann.name.as_str() {
                "PathParam" => {
                    path_params.push(Param {
                        name: ann.value.clone().unwrap_or_else(|| param_name.clone()),
                        param_type: param_type.clone(),
                        required: true,
                    });
                    matched = true;
                }
                "QueryParam" => {
                    query_params.push(Param {
                        name: ann.value.clone().unwrap_or_else(|| param_name.clone()),
                        param_type: param_type.clone(),
                        required: false,
                    });
                    matched = true;
                }
                _ => {}
            }
        }

        // Parameter without JAX-RS annotation = request body (for POST/PUT)
        if !matched && annotations.is_empty() {
            body = Some(TypeRef {
                name: clean_generic_type(&param_type),
                fields: BTreeMap::new(),
            });
        }
    }

    if body.is_none() && path_params.is_empty() && query_params.is_empty() {
        return None;
    }

    Some(EndpointInput {
        body,
        path_params,
        query_params,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ParsedFile;

    fn parse_file(source: &str, path: &str) -> ParsedFile {
        ParsedFile::parse(source.to_string(), path.to_string()).unwrap()
    }

    fn empty_ctx() -> AnalysisContext {
        AnalysisContext::new(vec![])
    }

    #[test]
    fn test_simple_rest_controller() {
        let source = r#"
import org.springframework.web.bind.annotation.*;

@RestController
@RequestMapping("/api/users")
public class UserController {

    @GetMapping
    public List<User> getAll() {
        return userService.findAll();
    }

    @GetMapping("/{id}")
    public User getById(@PathVariable Long id) {
        return userService.findById(id);
    }

    @PostMapping
    public User create(@Valid @RequestBody CreateUserRequest request) {
        return userService.create(request);
    }

    @DeleteMapping("/{id}")
    public void delete(@PathVariable Long id) {
        userService.delete(id);
    }
}
"#;

        let file = parse_file(source, "src/UserController.java");
        let capability = extract(&file, &empty_ctx()).unwrap();

        assert_eq!(capability.name, "user");
        assert_eq!(capability.endpoints.len(), 4);

        // GET /api/users
        let get_all = &capability.endpoints[0];
        assert!(matches!(get_all.method, HttpMethod::Get));
        assert_eq!(get_all.path, "/api/users");

        // GET /api/users/{id}
        let get_by_id = &capability.endpoints[1];
        assert!(matches!(get_by_id.method, HttpMethod::Get));
        assert_eq!(get_by_id.path, "/api/users/{id}");
        assert_eq!(get_by_id.input.as_ref().unwrap().path_params.len(), 1);

        // POST /api/users
        let create = &capability.endpoints[2];
        assert!(matches!(create.method, HttpMethod::Post));
        assert!(create.input.as_ref().unwrap().body.is_some());

        // DELETE /api/users/{id}
        let delete = &capability.endpoints[3];
        assert!(matches!(delete.method, HttpMethod::Delete));
    }

    #[test]
    fn test_throw_response_status_exception() {
        let source = r#"
@RestController
@RequestMapping("/api/users")
public class UserController {

    @GetMapping("/{id}")
    public User getById(@PathVariable Long id) {
        return userRepository.findById(id)
            .orElseThrow(() -> new ResponseStatusException(HttpStatus.NOT_FOUND, "User not found"));
    }
}
"#;

        let file = parse_file(source, "src/UserController.java");
        let capability = extract(&file, &empty_ctx()).unwrap();
        let endpoint = &capability.endpoints[0];

        assert_eq!(endpoint.behaviors.len(), 2);
        assert_eq!(endpoint.behaviors[0].name, "success");
        assert_eq!(endpoint.behaviors[0].returns.status, 200);
        assert_eq!(endpoint.behaviors[1].name, "not-found");
        assert_eq!(endpoint.behaviors[1].returns.status, 404);
    }

    #[test]
    fn test_throw_custom_exception() {
        let source = r#"
@RestController
@RequestMapping("/api/orders")
public class OrderController {

    @GetMapping("/{id}")
    public Order getById(@PathVariable Long id) {
        Order order = orderRepository.findById(id);
        if (order == null) {
            throw new OrderNotFoundException("Order not found: " + id);
        }
        return order;
    }
}
"#;

        let file = parse_file(source, "src/OrderController.java");
        let capability = extract(&file, &empty_ctx()).unwrap();
        let endpoint = &capability.endpoints[0];

        assert!(endpoint.behaviors.iter().any(|b| b.name == "not-found"));
    }

    #[test]
    fn test_response_entity_error_patterns() {
        let source = r#"
@RestController
@RequestMapping("/api/items")
public class ItemController {

    @GetMapping("/{id}")
    public ResponseEntity<Item> getById(@PathVariable Long id) {
        Item item = itemService.findById(id);
        if (item == null) {
            return ResponseEntity.notFound().build();
        }
        return ResponseEntity.ok(item);
    }

    @PostMapping
    public ResponseEntity<Item> create(@RequestBody Item item) {
        if (itemService.exists(item.getName())) {
            return ResponseEntity.status(HttpStatus.CONFLICT).body(null);
        }
        return ResponseEntity.status(HttpStatus.CREATED).body(itemService.save(item));
    }
}
"#;

        let file = parse_file(source, "src/ItemController.java");
        let capability = extract(&file, &empty_ctx()).unwrap();

        // GET endpoint: success + not-found
        let get_endpoint = &capability.endpoints[0];
        assert!(get_endpoint.behaviors.iter().any(|b| b.name == "success"));
        assert!(
            get_endpoint
                .behaviors
                .iter()
                .any(|b| b.name == "not-found" && b.returns.status == 404)
        );

        // POST endpoint: success + conflict
        let post_endpoint = &capability.endpoints[1];
        assert!(
            post_endpoint
                .behaviors
                .iter()
                .any(|b| b.name == "conflict" && b.returns.status == 409)
        );
    }

    #[test]
    fn test_response_status_annotation() {
        let source = r#"
@RestController
@RequestMapping("/api/events")
public class EventController {

    @PostMapping
    @ResponseStatus(HttpStatus.CREATED)
    public Event create(@RequestBody Event event) {
        return eventService.save(event);
    }

    @DeleteMapping("/{id}")
    @ResponseStatus(HttpStatus.NO_CONTENT)
    public void delete(@PathVariable Long id) {
        eventService.delete(id);
    }
}
"#;

        let file = parse_file(source, "src/EventController.java");
        let capability = extract(&file, &empty_ctx()).unwrap();

        let create = &capability.endpoints[0];
        assert_eq!(create.behaviors[0].returns.status, 201);

        let delete = &capability.endpoints[1];
        assert_eq!(delete.behaviors[0].returns.status, 204);
    }

    #[test]
    fn test_exception_handler() {
        let source = r#"
@RestController
@RequestMapping("/api/products")
public class ProductController {

    @GetMapping("/{id}")
    public Product getById(@PathVariable Long id) {
        return productService.findById(id);
    }

    @ExceptionHandler(ProductNotFoundException.class)
    @ResponseStatus(HttpStatus.NOT_FOUND)
    public ErrorResponse handleNotFound(ProductNotFoundException ex) {
        return new ErrorResponse(ex.getMessage());
    }

    @ExceptionHandler(DuplicateProductException.class)
    @ResponseStatus(HttpStatus.CONFLICT)
    public ErrorResponse handleDuplicate(DuplicateProductException ex) {
        return new ErrorResponse(ex.getMessage());
    }
}
"#;

        let file = parse_file(source, "src/ProductController.java");
        let capability = extract(&file, &empty_ctx()).unwrap();

        let get_endpoint = &capability.endpoints[0];
        // Should have: success, not-found (from ExceptionHandler), conflict (from ExceptionHandler)
        assert!(get_endpoint.behaviors.iter().any(|b| b.name == "success"));
        assert!(
            get_endpoint
                .behaviors
                .iter()
                .any(|b| b.name == "not-found" && b.returns.status == 404)
        );
        assert!(
            get_endpoint
                .behaviors
                .iter()
                .any(|b| b.name == "conflict" && b.returns.status == 409)
        );
    }

    #[test]
    fn test_bean_validation_deep_extraction() {
        let dto_source = r#"
public class CreateUserRequest {
    @NotBlank
    @Size(min = 2, max = 50)
    private String name;

    @NotBlank
    @Email
    private String email;

    @Size(min = 8, max = 100)
    private String password;

    @Min(18)
    private int age;

    private String bio;
}
"#;

        let controller_source = r#"
@RestController
@RequestMapping("/api/users")
public class UserController {

    @PostMapping
    public User create(@Valid @RequestBody CreateUserRequest request) {
        return userService.create(request);
    }
}
"#;

        let dto_file = parse_file(dto_source, "src/dto/CreateUserRequest.java");
        let controller_file = parse_file(controller_source, "src/UserController.java");

        let ctx = AnalysisContext::new(vec![dto_file]);
        let capability = extract(&controller_file, &ctx).unwrap();
        let endpoint = &capability.endpoints[0];

        // Body should have resolved fields
        let body = endpoint.input.as_ref().unwrap().body.as_ref().unwrap();
        assert_eq!(body.name, "CreateUserRequest");
        assert!(body.fields.contains_key("name"));
        assert!(body.fields.contains_key("email"));
        assert!(body.fields.contains_key("password"));
        assert!(body.fields.contains_key("age"));
        assert!(body.fields.contains_key("bio"));

        // Validation rules should be field-level, not just @Valid
        assert!(!endpoint.validation.is_empty());

        // Check name field has @NotBlank and @Size
        let name_rule = endpoint
            .validation
            .iter()
            .find(|r| r.field == "name")
            .expect("should have validation for 'name'");
        assert!(name_rule.constraints.contains(&"@NotBlank".to_string()));
        assert!(name_rule.constraints.iter().any(|c| c.contains("@Size")));

        // Check email field has @NotBlank and @Email
        let email_rule = endpoint
            .validation
            .iter()
            .find(|r| r.field == "email")
            .expect("should have validation for 'email'");
        assert!(email_rule.constraints.contains(&"@NotBlank".to_string()));
        assert!(email_rule.constraints.contains(&"@Email".to_string()));

        // Check age has @Min
        let age_rule = endpoint
            .validation
            .iter()
            .find(|r| r.field == "age")
            .expect("should have validation for 'age'");
        assert!(age_rule.constraints.iter().any(|c| c.contains("@Min")));

        // bio has no validation, should not appear
        assert!(endpoint.validation.iter().all(|r| r.field != "bio"));
    }

    #[test]
    fn test_cors_and_rate_limiting() {
        let source = r#"
@RestController
@RequestMapping("/api/public")
@CrossOrigin(origins = "https://example.com")
public class PublicController {

    @GetMapping("/data")
    public Data getData() {
        return dataService.getPublic();
    }

    @PostMapping("/submit")
    @RateLimiter(name = "submitLimit")
    public Result submit(@RequestBody Payload payload) {
        return dataService.process(payload);
    }
}
"#;

        let file = parse_file(source, "src/PublicController.java");
        let capability = extract(&file, &empty_ctx()).unwrap();

        // GET: should inherit class-level CORS
        let get_endpoint = &capability.endpoints[0];
        let sec = get_endpoint.security.as_ref().unwrap();
        assert!(sec.cors.as_ref().unwrap().contains("https://example.com"));

        // POST: should have rate limit + inherited CORS
        let post_endpoint = &capability.endpoints[1];
        let sec = post_endpoint.security.as_ref().unwrap();
        assert!(sec.cors.as_ref().unwrap().contains("https://example.com"));
        assert!(sec.rate_limit.as_ref().unwrap().contains("submitLimit"));
    }

    // ==================== JAX-RS Tests ====================

    #[test]
    fn test_jaxrs_resource_interface() {
        let source = r#"
import javax.ws.rs.*;
import javax.ws.rs.core.MediaType;

@Path("account")
public interface AccountResource {

    @GET
    @Path("/getAll")
    @Produces(MediaType.APPLICATION_JSON)
    List<AccountBTO> getAll();

    @GET
    @Path("/getByUsername/{username}")
    @Produces(MediaType.APPLICATION_JSON)
    AccountBTO getByUsername(@PathParam("username") String username);

    @POST
    @Path("/create")
    @Consumes(MediaType.APPLICATION_JSON)
    void create(AccountBTO account);

    @DELETE
    @Path("/delete/{id}")
    void delete(@PathParam("id") Long id);
}
"#;

        let file = parse_file(source, "src/AccountResource.java");
        let capability = extract(&file, &empty_ctx()).unwrap();

        assert_eq!(capability.name, "account");
        assert_eq!(capability.endpoints.len(), 4);

        // GET account/getAll
        let get_all = &capability.endpoints[0];
        assert!(matches!(get_all.method, HttpMethod::Get));
        assert_eq!(get_all.path, "account/getAll");

        // GET account/getByUsername/{username}
        let get_by = &capability.endpoints[1];
        assert!(matches!(get_by.method, HttpMethod::Get));
        assert_eq!(get_by.input.as_ref().unwrap().path_params.len(), 1);
        assert_eq!(
            get_by.input.as_ref().unwrap().path_params[0].name,
            "username"
        );

        // POST account/create — body param
        let create = &capability.endpoints[2];
        assert!(matches!(create.method, HttpMethod::Post));
        assert!(create.input.as_ref().unwrap().body.is_some());

        // DELETE
        let delete = &capability.endpoints[3];
        assert!(matches!(delete.method, HttpMethod::Delete));
    }

    #[test]
    fn test_jaxrs_security() {
        let source = r#"
import javax.ws.rs.*;
import javax.annotation.security.*;

@Path("admin")
@RolesAllowed("ADMIN")
public interface AdminResource {

    @GET
    @Path("/secrets")
    String getSecrets();

    @GET
    @Path("/public")
    @PermitAll
    String getPublic();
}
"#;

        let file = parse_file(source, "src/AdminResource.java");
        let capability = extract(&file, &empty_ctx()).unwrap();

        // getSecrets: inherits class-level @RolesAllowed
        let secrets = &capability.endpoints[0];
        let sec = secrets.security.as_ref().unwrap();
        assert_eq!(sec.authentication.as_deref(), Some("required"));
        assert!(sec.roles.iter().any(|r| r.contains("ADMIN")));

        // getPublic: @PermitAll overrides
        let public_ep = &capability.endpoints[1];
        assert!(public_ep.security.is_none());
    }

    #[test]
    fn test_jaxrs_query_params() {
        let source = r#"
@Path("search")
public interface SearchResource {

    @GET
    @Path("/results")
    List<Result> search(@QueryParam("q") String query, @QueryParam("page") int page);
}
"#;

        let file = parse_file(source, "src/SearchResource.java");
        let capability = extract(&file, &empty_ctx()).unwrap();

        let endpoint = &capability.endpoints[0];
        assert_eq!(endpoint.input.as_ref().unwrap().query_params.len(), 2);
        assert_eq!(endpoint.input.as_ref().unwrap().query_params[0].name, "q");
        assert_eq!(
            endpoint.input.as_ref().unwrap().query_params[1].name,
            "page"
        );
    }
}
