use crate::parser::parse_java;
use crate::JavaAnalysisError;
use pecto_core::model::*;
use std::collections::BTreeMap;
use tree_sitter::Node;

/// Extract a Capability from a Java source file if it contains Spring controllers.
pub fn extract_capability(
    source: &str,
    file_path: &str,
) -> Result<Option<Capability>, JavaAnalysisError> {
    let tree = parse_java(source).ok_or_else(|| {
        JavaAnalysisError::ParseError(file_path.to_string(), "Failed to parse Java".to_string())
    })?;

    let root = tree.root_node();
    let source_bytes = source.as_bytes();

    // Find class declarations
    for i in 0..root.named_child_count() {
        let node = root.named_child(i).unwrap();
        if node.kind() == "class_declaration" {
            let annotations = collect_annotations(&node, source_bytes);

            // Check if this is a Spring REST controller
            let is_controller = annotations
                .iter()
                .any(|a| a.name == "RestController" || a.name == "Controller");

            if is_controller {
                let class_name = get_class_name(&node, source_bytes);
                let base_path = extract_request_mapping_path(&annotations);
                let capability_name = to_kebab_case(&class_name.replace("Controller", ""));

                let mut capability = Capability::new(capability_name, file_path.to_string());

                // Extract endpoints from methods
                extract_endpoints_from_class(
                    &node,
                    source_bytes,
                    &base_path,
                    &mut capability,
                );

                return Ok(Some(capability));
            }
        }
    }

    Ok(None)
}

/// Collect all annotations from a node's modifiers.
fn collect_annotations<'a>(node: &Node<'a>, source: &[u8]) -> Vec<AnnotationInfo> {
    let mut annotations = Vec::new();

    // Check modifiers node which contains annotations
    for i in 0..node.named_child_count() {
        let child = node.named_child(i).unwrap();
        if child.kind() == "modifiers" {
            for j in 0..child.named_child_count() {
                let modifier = child.named_child(j).unwrap();
                if (modifier.kind() == "marker_annotation" || modifier.kind() == "annotation")
                    && let Some(ann) = parse_annotation(&modifier, source) {
                        annotations.push(ann);
                    }
            }
        }
    }

    annotations
}

#[derive(Debug)]
struct AnnotationInfo {
    name: String,
    arguments: BTreeMap<String, String>,
    value: Option<String>,
}

fn parse_annotation(node: &Node, source: &[u8]) -> Option<AnnotationInfo> {
    let mut name = String::new();
    let mut arguments = BTreeMap::new();
    let mut value = None;

    for i in 0..node.named_child_count() {
        let child = node.named_child(i).unwrap();
        match child.kind() {
            "identifier" | "scoped_identifier" => {
                name = node_text(&child, source);
            }
            "annotation_argument_list" => {
                for j in 0..child.named_child_count() {
                    let arg = child.named_child(j).unwrap();
                    match arg.kind() {
                        "element_value_pair" => {
                            let key = arg
                                .child_by_field_name("key")
                                .map(|k| node_text(&k, source))
                                .unwrap_or_default();
                            let val = arg
                                .child_by_field_name("value")
                                .map(|v| node_text(&v, source))
                                .unwrap_or_default();
                            arguments.insert(key, clean_string_literal(&val));
                        }
                        _ => {
                            value = Some(clean_string_literal(&node_text(&arg, source)));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    if name.is_empty() {
        return None;
    }

    Some(AnnotationInfo {
        name,
        arguments,
        value,
    })
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
) {
    let body = match class_node.child_by_field_name("body") {
        Some(b) => b,
        None => return,
    };

    for i in 0..body.named_child_count() {
        let member = body.named_child(i).unwrap();
        if member.kind() == "method_declaration" {
            let annotations = collect_annotations(&member, source);

            if let Some(endpoint) = extract_endpoint_from_method(&member, source, &annotations, base_path) {
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
) -> Option<Endpoint> {
    // Determine HTTP method and path from annotations
    let (http_method, method_path) = extract_http_method_and_path(annotations)?;

    let full_path = if base_path.is_empty() {
        method_path
    } else {
        format!("{}{}", base_path.trim_end_matches('/'), method_path)
    };

    // Extract method parameters for input
    let input = extract_method_input(method_node, source);

    // Extract validation annotations from parameters
    let validation = extract_validation_rules(method_node, source);

    // Extract return type info
    let return_type = method_node
        .child_by_field_name("type")
        .map(|t| node_text(&t, source));

    // Extract security annotations
    let security = extract_security_config(annotations);

    // Build basic behavior based on return type
    let behaviors = vec![Behavior {
        name: "success".to_string(),
        condition: None,
        returns: ResponseSpec {
            status: default_success_status(&http_method),
            body: return_type.map(|t| TypeRef {
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
        validation,
        behaviors,
        security,
    })
}

fn extract_http_method_and_path(annotations: &[AnnotationInfo]) -> Option<(HttpMethod, String)> {
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

fn extract_path_from_annotation(ann: &AnnotationInfo) -> String {
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

fn extract_method_input(method_node: &Node, source: &[u8]) -> Option<EndpointInput> {
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
                    body = Some(TypeRef {
                        name: clean_generic_type(&param_type),
                        fields: BTreeMap::new(),
                    });
                }
                "PathVariable" => {
                    path_params.push(Param {
                        name: ann
                            .value
                            .clone()
                            .unwrap_or_else(|| param_name.clone()),
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
                        name: ann
                            .value
                            .clone()
                            .unwrap_or_else(|| param_name.clone()),
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

fn extract_validation_rules(method_node: &Node, source: &[u8]) -> Vec<ValidationRule> {
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
        let has_valid = annotations.iter().any(|a| a.name == "Valid" || a.name == "Validated");

        if has_valid {
            let param_name = param
                .child_by_field_name("name")
                .map(|n| node_text(&n, source))
                .unwrap_or_default();
            rules.push(ValidationRule {
                field: param_name,
                constraints: vec!["@Valid".to_string()],
            });
        }
    }

    rules
}

fn extract_security_config(annotations: &[AnnotationInfo]) -> Option<SecurityConfig> {
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

    if auth.is_none() && roles.is_empty() {
        return None;
    }

    Some(SecurityConfig {
        authentication: auth,
        roles,
        rate_limit: None,
    })
}

fn get_class_name(node: &Node, source: &[u8]) -> String {
    node.child_by_field_name("name")
        .map(|n| node_text(&n, source))
        .unwrap_or_else(|| "Unknown".to_string())
}

// --- Utility functions ---

fn node_text(node: &Node, source: &[u8]) -> String {
    node.utf8_text(source).unwrap_or("").to_string()
}

fn clean_string_literal(s: &str) -> String {
    s.trim_matches('"').trim_matches('\'').to_string()
}

fn clean_generic_type(t: &str) -> String {
    // ResponseEntity<User> -> User
    if let Some(start) = t.find('<')
        && let Some(end) = t.rfind('>') {
            return t[start + 1..end].to_string();
        }
    t.to_string()
}

fn to_kebab_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            result.push('-');
        }
        result.push(c.to_ascii_lowercase());
    }
    result
}

fn default_success_status(method: &HttpMethod) -> u16 {
    match method {
        HttpMethod::Post => 201,
        HttpMethod::Delete => 204,
        _ => 200,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

        let capability = extract_capability(source, "src/UserController.java")
            .unwrap()
            .unwrap();

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
}
