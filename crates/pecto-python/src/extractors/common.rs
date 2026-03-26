use tree_sitter::Node;

/// Parsed decorator info for Python `@decorator(args)`.
#[derive(Debug)]
pub struct DecoratorInfo {
    pub name: String,
    pub full_name: String, // e.g. "app.get" or "router.post"
    pub args: Vec<String>,
}

/// Collect decorators from a decorated_definition or directly from a node.
pub fn collect_decorators(node: &Node, source: &[u8]) -> Vec<DecoratorInfo> {
    let mut decorators = Vec::new();

    // If this is a decorated_definition, decorators are children
    if node.kind() == "decorated_definition" {
        for i in 0..node.named_child_count() {
            let child = node.named_child(i).unwrap();
            if child.kind() == "decorator"
                && let Some(info) = parse_decorator(&child, source)
            {
                decorators.push(info);
            }
        }
    }

    decorators
}

/// Parse a single decorator node.
fn parse_decorator(node: &Node, source: &[u8]) -> Option<DecoratorInfo> {
    // Decorator structure: @ expression
    // The expression can be: identifier, attribute (a.b), or call (a.b(...))
    let mut full_name = String::new();
    let mut args = Vec::new();

    for i in 0..node.named_child_count() {
        let child = node.named_child(i).unwrap();
        match child.kind() {
            "identifier" => {
                full_name = node_text(&child, source);
            }
            "attribute" => {
                full_name = node_text(&child, source);
            }
            "call" => {
                // call has function + arguments
                if let Some(func) = child.child_by_field_name("function") {
                    full_name = node_text(&func, source);
                }
                if let Some(arg_list) = child.child_by_field_name("arguments") {
                    for j in 0..arg_list.named_child_count() {
                        let arg = arg_list.named_child(j).unwrap();
                        args.push(node_text(&arg, source));
                    }
                }
            }
            _ => {}
        }
    }

    if full_name.is_empty() {
        return None;
    }

    // Extract simple name: "app.get" → "get", "router.post" → "post"
    let name = full_name
        .rsplit('.')
        .next()
        .unwrap_or(&full_name)
        .to_string();

    Some(DecoratorInfo {
        name,
        full_name,
        args,
    })
}

/// Extract text content from a tree-sitter node.
pub fn node_text(node: &Node, source: &[u8]) -> String {
    node.utf8_text(source).unwrap_or("").to_string()
}

/// Remove surrounding quotes from a string literal.
pub fn clean_string_literal(s: &str) -> String {
    s.trim_matches('"')
        .trim_matches('\'')
        .trim_start_matches("f\"")
        .trim_start_matches("f'")
        .to_string()
}

/// Convert PascalCase/snake_case to kebab-case.
pub fn to_kebab_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c == '_' {
            result.push('-');
        } else if c.is_uppercase() && i > 0 {
            result.push('-');
            result.push(c.to_ascii_lowercase());
        } else {
            result.push(c.to_ascii_lowercase());
        }
    }
    result
}

/// Get the function/class name from a definition node.
pub fn get_def_name(node: &Node, source: &[u8]) -> String {
    node.child_by_field_name("name")
        .map(|n| node_text(&n, source))
        .unwrap_or_else(|| "unknown".to_string())
}

/// Get the inner definition from a decorated_definition node.
pub fn get_inner_definition<'a>(node: &'a Node<'a>) -> Option<Node<'a>> {
    if node.kind() == "decorated_definition" {
        node.named_children(&mut node.walk())
            .find(|c| c.kind() == "function_definition" || c.kind() == "class_definition")
    } else {
        Some(*node)
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
    fn test_parse_python_decorators() {
        let source = r#"
from fastapi import APIRouter

router = APIRouter()

@router.get("/users/{user_id}")
async def get_user(user_id: int):
    return {"user_id": user_id}

@router.post("/users")
async def create_user(user: UserCreate):
    return user
"#;

        let file = parse_file(source, "routes.py");
        let root = file.tree.root_node();
        let src = file.source.as_bytes();

        let mut found = Vec::new();
        for i in 0..root.named_child_count() {
            let node = root.named_child(i).unwrap();
            if node.kind() == "decorated_definition" {
                let decorators = collect_decorators(&node, src);
                for d in &decorators {
                    found.push((d.name.clone(), d.full_name.clone(), d.args.clone()));
                }
            }
        }

        assert_eq!(found.len(), 2);
        assert_eq!(found[0].0, "get"); // name
        assert_eq!(found[0].1, "router.get"); // full_name
        assert!(found[0].2[0].contains("/users/")); // first arg is path

        assert_eq!(found[1].0, "post");
        assert_eq!(found[1].1, "router.post");
    }

    #[test]
    fn test_to_kebab_case() {
        assert_eq!(to_kebab_case("UserService"), "user-service");
        assert_eq!(to_kebab_case("user_service"), "user-service");
        assert_eq!(to_kebab_case("get_users"), "get-users");
    }

    #[test]
    fn test_clean_string_literal() {
        assert_eq!(clean_string_literal("\"hello\""), "hello");
        assert_eq!(clean_string_literal("'/api/users'"), "/api/users");
    }
}
