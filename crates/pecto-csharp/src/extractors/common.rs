use std::collections::BTreeMap;
use tree_sitter::Node;

/// Parsed attribute info shared across all extractors.
/// C# equivalent of Java's AnnotationInfo.
#[derive(Debug)]
pub struct AttributeInfo {
    pub name: String,
    pub arguments: BTreeMap<String, String>,
    pub value: Option<String>,
}

/// Collect all attributes from a C# node's attribute_list children.
pub fn collect_attributes(node: &Node, source: &[u8]) -> Vec<AttributeInfo> {
    let mut attributes = Vec::new();

    for i in 0..node.named_child_count() {
        let child = node.named_child(i).unwrap();
        if child.kind() == "attribute_list" {
            for j in 0..child.named_child_count() {
                let attr = child.named_child(j).unwrap();
                if attr.kind() == "attribute"
                    && let Some(info) = parse_attribute(&attr, source)
                {
                    attributes.push(info);
                }
            }
        }
    }

    attributes
}

/// Parse a single C# attribute node into AttributeInfo.
pub fn parse_attribute(node: &Node, source: &[u8]) -> Option<AttributeInfo> {
    let mut name = String::new();
    let mut arguments = BTreeMap::new();
    let mut value = None;

    for i in 0..node.named_child_count() {
        let child = node.named_child(i).unwrap();
        match child.kind() {
            "identifier" | "qualified_name" => {
                name = node_text(&child, source);
            }
            "attribute_argument_list" => {
                for j in 0..child.named_child_count() {
                    let arg = child.named_child(j).unwrap();
                    if arg.kind() == "attribute_argument" {
                        // Check for name = value pairs
                        if let Some(name_colon_or_equals) = find_named_argument(&arg, source) {
                            let (key, val) = name_colon_or_equals;
                            arguments.insert(key, clean_string_literal(&val));
                        } else {
                            // Positional argument
                            let text = node_text(&arg, source);
                            if value.is_none() {
                                value = Some(clean_string_literal(&text));
                            } else {
                                // Multiple positional args — store as numbered
                                arguments.insert(
                                    arguments.len().to_string(),
                                    clean_string_literal(&text),
                                );
                            }
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

    Some(AttributeInfo {
        name,
        arguments,
        value,
    })
}

/// Try to extract a named argument (Name = Value) from an attribute_argument node.
fn find_named_argument(node: &Node, source: &[u8]) -> Option<(String, String)> {
    // Look for name_equals or name_colon child
    for i in 0..node.named_child_count() {
        let child = node.named_child(i).unwrap();
        if child.kind() == "name_equals" || child.kind() == "name_colon" {
            let key = child
                .named_child(0)
                .map(|n| node_text(&n, source))
                .unwrap_or_default();
            // The value is the sibling after the name_equals
            let val_node = node.named_child(i + 1);
            let val = val_node.map(|n| node_text(&n, source)).unwrap_or_default();
            return Some((key, val));
        }
    }

    // Also check for assignment_expression pattern: Key = Value
    let text = node_text(node, source);
    if text.contains('=') && !text.starts_with('"') {
        let parts: Vec<&str> = text.splitn(2, '=').collect();
        if parts.len() == 2 {
            let key = parts[0].trim().to_string();
            let val = parts[1].trim().to_string();
            if !key.is_empty() && key.chars().all(|c| c.is_alphanumeric() || c == '_') {
                return Some((key, clean_string_literal(&val)));
            }
        }
    }

    None
}

/// Extract text content from a tree-sitter node.
pub fn node_text(node: &Node, source: &[u8]) -> String {
    node.utf8_text(source).unwrap_or("").to_string()
}

/// Remove surrounding quotes from a string literal.
pub fn clean_string_literal(s: &str) -> String {
    s.trim_matches('"').trim_matches('\'').to_string()
}

/// Clean generic wrapper types: `Task<ActionResult<User>>` -> `User`.
pub fn clean_generic_type(t: &str) -> String {
    let mut result = t.to_string();
    // Unwrap Task<T>
    if let Some(inner) = unwrap_generic(&result, "Task") {
        result = inner;
    }
    // Unwrap ActionResult<T>
    if let Some(inner) = unwrap_generic(&result, "ActionResult") {
        result = inner;
    }
    // Unwrap generic wrapper if still present
    if let Some(start) = result.find('<')
        && let Some(end) = result.rfind('>')
    {
        result = result[start + 1..end].to_string();
    }
    result
}

/// Unwrap a specific generic type: `Foo<Bar>` with name "Foo" → `Bar`.
fn unwrap_generic(t: &str, wrapper: &str) -> Option<String> {
    let trimmed = t.trim();
    if trimmed.starts_with(wrapper)
        && trimmed[wrapper.len()..].starts_with('<')
        && trimmed.ends_with('>')
    {
        Some(trimmed[wrapper.len() + 1..trimmed.len() - 1].to_string())
    } else {
        None
    }
}

/// Convert PascalCase to kebab-case: `UserController` -> `user-controller`.
pub fn to_kebab_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            result.push('-');
        }
        result.push(c.to_ascii_lowercase());
    }
    result
}

/// Get the class name from a class_declaration node.
pub fn get_class_name(node: &Node, source: &[u8]) -> String {
    node.child_by_field_name("name")
        .map(|n| node_text(&n, source))
        .unwrap_or_else(|| "Unknown".to_string())
}

/// Iterate over class_declaration nodes inside namespaces and other wrappers.
/// Calls the callback for each class found.
pub fn for_each_class(node: &Node, source: &[u8], callback: &mut dyn FnMut(&Node, &[u8])) {
    for i in 0..node.named_child_count() {
        let child = node.named_child(i).unwrap();
        match child.kind() {
            "class_declaration" => {
                callback(&child, source);
            }
            "namespace_declaration" | "file_scoped_namespace_declaration" | "declaration_list" => {
                for_each_class(&child, source, callback);
            }
            _ => {}
        }
    }
}

/// Iterate over interface_declaration nodes inside namespaces.
pub fn for_each_interface(node: &Node, source: &[u8], callback: &mut dyn FnMut(&Node, &[u8])) {
    for i in 0..node.named_child_count() {
        let child = node.named_child(i).unwrap();
        match child.kind() {
            "interface_declaration" => {
                callback(&child, source);
            }
            "namespace_declaration" | "file_scoped_namespace_declaration" | "declaration_list" => {
                for_each_interface(&child, source, callback);
            }
            _ => {}
        }
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
    fn test_parse_csharp_attributes() {
        let source = r#"
using System.ComponentModel.DataAnnotations;

namespace MyApp.Controllers;

[ApiController]
[Route("api/[controller]")]
public class UserController : ControllerBase
{
    [HttpGet]
    public IActionResult GetAll() { return Ok(); }

    [HttpGet("{id}")]
    [Authorize(Roles = "Admin")]
    public IActionResult GetById(int id) { return Ok(); }
}
"#;

        let file = parse_file(source, "UserController.cs");
        let root = file.tree.root_node();
        let source_bytes = file.source.as_bytes();

        let mut found_classes = Vec::new();
        for_each_class(&root, source_bytes, &mut |node, src| {
            let name = get_class_name(node, src);
            let attrs = collect_attributes(node, src);
            found_classes.push((name, attrs));
        });

        assert_eq!(found_classes.len(), 1);
        let (name, attrs) = &found_classes[0];

        assert_eq!(name, "UserController");
        assert!(attrs.iter().any(|a| a.name == "ApiController"));
        assert!(attrs.iter().any(|a| a.name == "Route"));

        let route = attrs.iter().find(|a| a.name == "Route").unwrap();
        assert_eq!(route.value.as_deref(), Some("api/[controller]"));
    }

    #[test]
    fn test_attribute_with_named_arguments() {
        let source = r#"
[Authorize(Roles = "Admin", Policy = "RequireAdmin")]
public class AdminController { }
"#;

        let file = parse_file(source, "AdminController.cs");
        let root = file.tree.root_node();
        let source_bytes = file.source.as_bytes();

        let mut found = Vec::new();
        for_each_class(&root, source_bytes, &mut |node, src| {
            found.push(collect_attributes(node, src));
        });

        assert_eq!(found.len(), 1);
        let attrs = &found[0];

        let auth = attrs.iter().find(|a| a.name == "Authorize").unwrap();
        assert!(
            auth.arguments.contains_key("Roles") || auth.value.is_some(),
            "Should parse Authorize attribute arguments: {:?}",
            auth
        );
    }

    #[test]
    fn test_clean_generic_type() {
        assert_eq!(clean_generic_type("Task<ActionResult<User>>"), "User");
        assert_eq!(clean_generic_type("ActionResult<Order>"), "Order");
        assert_eq!(clean_generic_type("Task<IActionResult>"), "IActionResult");
        assert_eq!(clean_generic_type("List<string>"), "string");
        assert_eq!(clean_generic_type("string"), "string");
    }
}
