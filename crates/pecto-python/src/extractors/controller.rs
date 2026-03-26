use super::common::*;
use crate::context::ParsedFile;
use pecto_core::model::*;
use std::collections::BTreeMap;

/// Extract endpoints from a Python file containing FastAPI, Flask, or Django routes.
pub fn extract(file: &ParsedFile) -> Option<Capability> {
    let root = file.tree.root_node();
    let source = file.source.as_bytes();

    let mut endpoints = Vec::new();

    for i in 0..root.named_child_count() {
        let node = root.named_child(i).unwrap();

        if node.kind() == "decorated_definition" {
            let decorators = collect_decorators(&node, source);
            let inner = match get_inner_definition(&node) {
                Some(n) => n,
                None => continue,
            };

            if inner.kind() == "function_definition" {
                // FastAPI/Flask route decorators
                for dec in &decorators {
                    if let Some(endpoint) = extract_route_endpoint(&inner, source, dec) {
                        endpoints.push(endpoint);
                    }
                }

                // Django REST Framework @api_view
                if let Some(endpoint) = extract_drf_api_view(&inner, source, &decorators) {
                    endpoints.push(endpoint);
                }
            }

            if inner.kind() == "class_definition" {
                // Django REST Framework ViewSets
                extract_drf_viewset(&inner, source, &decorators, &mut endpoints);
            }
        }
    }

    if endpoints.is_empty() {
        return None;
    }

    // Derive capability name from file
    let file_stem = file
        .path
        .rsplit('/')
        .next()
        .unwrap_or(&file.path)
        .trim_end_matches(".py");
    let capability_name = to_kebab_case(
        &file_stem
            .replace("_routes", "")
            .replace("_views", "")
            .replace("_router", "")
            .replace("_api", ""),
    );

    let mut capability = Capability::new(capability_name, file.path.clone());
    capability.endpoints = endpoints;
    Some(capability)
}

/// Extract endpoint from FastAPI/Flask route decorator.
fn extract_route_endpoint(
    func_node: &tree_sitter::Node,
    source: &[u8],
    decorator: &DecoratorInfo,
) -> Option<Endpoint> {
    let http_method = match decorator.name.as_str() {
        "get" | "GET" => HttpMethod::Get,
        "post" | "POST" => HttpMethod::Post,
        "put" | "PUT" => HttpMethod::Put,
        "delete" | "DELETE" => HttpMethod::Delete,
        "patch" | "PATCH" => HttpMethod::Patch,
        "route" => {
            // Flask @app.route — check methods kwarg
            extract_flask_method(&decorator.args)
        }
        _ => return None,
    };

    // Don't match standalone decorators that aren't route-like
    if !decorator.full_name.contains('.')
        && !matches!(
            decorator.name.as_str(),
            "get" | "post" | "put" | "delete" | "patch"
        )
    {
        return None;
    }

    // Extract path from first argument
    let path = decorator
        .args
        .first()
        .map(|a| clean_string_literal(a))
        .unwrap_or_default();

    if path.is_empty() && decorator.name != "route" {
        return None;
    }

    let _func_name = get_def_name(func_node, source);

    // Extract parameters from function signature
    let input = extract_function_params(func_node, source);

    // Check for security dependencies (FastAPI Depends)
    let security = extract_security(func_node, source);

    let behaviors = vec![Behavior {
        name: "success".to_string(),
        condition: None,
        returns: ResponseSpec {
            status: default_status(&http_method),
            body: extract_return_type(func_node, source),
        },
        side_effects: Vec::new(),
    }];

    Some(Endpoint {
        method: http_method,
        path,
        input,
        validation: Vec::new(),
        behaviors,
        security,
    })
}

/// Extract HTTP method from Flask @app.route(methods=["GET", "POST"])
fn extract_flask_method(args: &[String]) -> HttpMethod {
    for arg in args {
        if arg.contains("POST") {
            return HttpMethod::Post;
        }
        if arg.contains("PUT") {
            return HttpMethod::Put;
        }
        if arg.contains("DELETE") {
            return HttpMethod::Delete;
        }
        if arg.contains("PATCH") {
            return HttpMethod::Patch;
        }
    }
    HttpMethod::Get
}

/// Extract endpoint from DRF @api_view decorator.
fn extract_drf_api_view(
    func_node: &tree_sitter::Node,
    source: &[u8],
    decorators: &[DecoratorInfo],
) -> Option<Endpoint> {
    let api_view = decorators.iter().find(|d| d.name == "api_view")?;

    let method = if !api_view.args.is_empty() {
        extract_flask_method(&api_view.args)
    } else {
        HttpMethod::Get
    };

    let func_name = get_def_name(func_node, source);
    let path = format!("/{}", func_name.replace('_', "-"));

    Some(Endpoint {
        method,
        path,
        input: None,
        validation: Vec::new(),
        behaviors: vec![Behavior {
            name: "success".to_string(),
            condition: None,
            returns: ResponseSpec {
                status: 200,
                body: None,
            },
            side_effects: Vec::new(),
        }],
        security: None,
    })
}

/// Extract CRUD endpoints from DRF ViewSet class.
fn extract_drf_viewset(
    class_node: &tree_sitter::Node,
    source: &[u8],
    _decorators: &[DecoratorInfo],
    endpoints: &mut Vec<Endpoint>,
) {
    // Check if class inherits from known ViewSet bases
    let bases = get_class_bases(class_node, source);
    let is_viewset = bases.iter().any(|b| {
        b.contains("ViewSet")
            || b.contains("ModelViewSet")
            || b.contains("GenericAPIView")
            || b.contains("APIView")
    });

    if !is_viewset {
        return;
    }

    let class_name = get_def_name(class_node, source);
    let base_path = format!(
        "/{}",
        to_kebab_case(&class_name.replace("ViewSet", "").replace("View", ""))
    );

    let is_model_viewset = bases.iter().any(|b| b.contains("ModelViewSet"));

    if is_model_viewset {
        // ModelViewSet provides standard CRUD
        let crud = [
            (HttpMethod::Get, format!("{}/", base_path), "list"),
            (HttpMethod::Post, format!("{}/", base_path), "create"),
            (HttpMethod::Get, format!("{}/:id/", base_path), "retrieve"),
            (HttpMethod::Put, format!("{}/:id/", base_path), "update"),
            (HttpMethod::Delete, format!("{}/:id/", base_path), "destroy"),
        ];
        for (method, path, _name) in crud {
            endpoints.push(Endpoint {
                method,
                path,
                input: None,
                validation: Vec::new(),
                behaviors: vec![Behavior {
                    name: "success".to_string(),
                    condition: None,
                    returns: ResponseSpec {
                        status: 200,
                        body: None,
                    },
                    side_effects: Vec::new(),
                }],
                security: None,
            });
        }
    }
}

fn get_class_bases(class_node: &tree_sitter::Node, source: &[u8]) -> Vec<String> {
    let mut bases = Vec::new();
    if let Some(arg_list) = class_node.child_by_field_name("superclasses") {
        for i in 0..arg_list.named_child_count() {
            let arg = arg_list.named_child(i).unwrap();
            bases.push(node_text(&arg, source));
        }
    }
    bases
}

fn extract_function_params(func_node: &tree_sitter::Node, source: &[u8]) -> Option<EndpointInput> {
    let params = func_node.child_by_field_name("parameters")?;
    let mut path_params = Vec::new();
    let mut body = None;

    for i in 0..params.named_child_count() {
        let param = params.named_child(i).unwrap();

        let param_name = match param.kind() {
            "typed_parameter" | "typed_default_parameter" => param
                .child_by_field_name("name")
                .map(|n| node_text(&n, source))
                .unwrap_or_default(),
            "identifier" => node_text(&param, source),
            _ => continue,
        };

        // Skip self, request, db, response
        if matches!(
            param_name.as_str(),
            "self" | "request" | "db" | "response" | "session"
        ) {
            continue;
        }

        // Check type annotation
        let type_ann = param
            .child_by_field_name("type")
            .map(|t| node_text(&t, source));

        if let Some(ref t) = type_ann {
            // If type looks like a model (PascalCase), it's a body
            if t.chars().next().is_some_and(|c| c.is_uppercase())
                && !t.starts_with("Optional")
                && !t.starts_with("int")
                && !t.starts_with("str")
                && !t.starts_with("float")
                && !t.starts_with("bool")
            {
                body = Some(TypeRef {
                    name: t.clone(),
                    fields: BTreeMap::new(),
                });
                continue;
            }
        }

        // Simple types are path params
        if !param_name.is_empty() {
            path_params.push(Param {
                name: param_name,
                param_type: type_ann.unwrap_or_else(|| "str".to_string()),
                required: true,
            });
        }
    }

    if body.is_none() && path_params.is_empty() {
        return None;
    }

    Some(EndpointInput {
        body,
        path_params,
        query_params: Vec::new(),
    })
}

fn extract_security(func_node: &tree_sitter::Node, source: &[u8]) -> Option<SecurityConfig> {
    let params = func_node.child_by_field_name("parameters")?;
    let text = node_text(&params, source);

    // FastAPI: Depends(get_current_user)
    if text.contains("Depends") && (text.contains("current_user") || text.contains("auth")) {
        return Some(SecurityConfig {
            authentication: Some("required".to_string()),
            roles: Vec::new(),
            rate_limit: None,
            cors: None,
        });
    }

    None
}

fn extract_return_type(func_node: &tree_sitter::Node, source: &[u8]) -> Option<TypeRef> {
    let return_type = func_node.child_by_field_name("return_type")?;
    let type_text = node_text(&return_type, source);

    if type_text == "None" || type_text.is_empty() {
        return None;
    }

    Some(TypeRef {
        name: type_text,
        fields: BTreeMap::new(),
    })
}

fn default_status(method: &HttpMethod) -> u16 {
    match method {
        HttpMethod::Post => 201,
        HttpMethod::Delete => 204,
        _ => 200,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ParsedFile;

    fn parse_file(source: &str, path: &str) -> ParsedFile {
        ParsedFile::parse(source.to_string(), path.to_string()).unwrap()
    }

    #[test]
    fn test_fastapi_routes() {
        let source = r#"
from fastapi import APIRouter, Depends

router = APIRouter()

@router.get("/users/{user_id}")
async def get_user(user_id: int) -> User:
    return db.get(user_id)

@router.post("/users")
async def create_user(user: UserCreate, current_user: User = Depends(get_current_user)):
    return db.create(user)

@router.delete("/users/{user_id}")
async def delete_user(user_id: int):
    db.delete(user_id)
"#;

        let file = parse_file(source, "routes/users.py");
        let capability = extract(&file).unwrap();

        assert_eq!(capability.name, "users");
        assert_eq!(capability.endpoints.len(), 3);

        let get = &capability.endpoints[0];
        assert!(matches!(get.method, HttpMethod::Get));
        assert_eq!(get.path, "/users/{user_id}");

        let post = &capability.endpoints[1];
        assert!(matches!(post.method, HttpMethod::Post));

        let delete = &capability.endpoints[2];
        assert!(matches!(delete.method, HttpMethod::Delete));
    }

    #[test]
    fn test_flask_routes() {
        let source = r#"
from flask import Blueprint

bp = Blueprint('items', __name__)

@bp.route("/items", methods=["GET"])
def list_items():
    return jsonify(items)

@bp.route("/items", methods=["POST"])
def create_item():
    return jsonify(item), 201
"#;

        let file = parse_file(source, "views/items.py");
        let capability = extract(&file).unwrap();

        assert_eq!(capability.name, "items");
        assert_eq!(capability.endpoints.len(), 2);

        assert!(matches!(capability.endpoints[0].method, HttpMethod::Get));
        assert!(matches!(capability.endpoints[1].method, HttpMethod::Post));
    }

    #[test]
    fn test_drf_api_view() {
        let source = r#"
from rest_framework.decorators import api_view

@api_view(['GET', 'POST'])
def user_list(request):
    pass
"#;

        let file = parse_file(source, "views/users.py");
        let capability = extract(&file).unwrap();

        assert_eq!(capability.endpoints.len(), 1);
    }

    #[test]
    fn test_no_routes() {
        let source = r#"
def helper_function():
    return 42

class Calculator:
    def add(self, a, b):
        return a + b
"#;

        let file = parse_file(source, "utils.py");
        assert!(extract(&file).is_none());
    }
}
