use std::collections::BTreeMap;
use tree_sitter::Node;

/// Parsed annotation info shared across all extractors.
#[derive(Debug)]
pub struct AnnotationInfo {
    pub name: String,
    pub arguments: BTreeMap<String, String>,
    pub value: Option<String>,
}

/// Collect all annotations from a node's modifiers.
pub fn collect_annotations(node: &Node, source: &[u8]) -> Vec<AnnotationInfo> {
    let mut annotations = Vec::new();

    for i in 0..node.named_child_count() {
        let child = node.named_child(i).unwrap();
        if child.kind() == "modifiers" {
            for j in 0..child.named_child_count() {
                let modifier = child.named_child(j).unwrap();
                if (modifier.kind() == "marker_annotation" || modifier.kind() == "annotation")
                    && let Some(ann) = parse_annotation(&modifier, source)
                {
                    annotations.push(ann);
                }
            }
        }
    }

    annotations
}

/// Parse a single annotation node into AnnotationInfo.
pub fn parse_annotation(node: &Node, source: &[u8]) -> Option<AnnotationInfo> {
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

/// Extract text content from a tree-sitter node.
pub fn node_text(node: &Node, source: &[u8]) -> String {
    node.utf8_text(source).unwrap_or("").to_string()
}

/// Remove surrounding quotes from a string literal.
pub fn clean_string_literal(s: &str) -> String {
    s.trim_matches('"').trim_matches('\'').to_string()
}

/// Clean generic wrapper types: `ResponseEntity<User>` -> `User`.
pub fn clean_generic_type(t: &str) -> String {
    if let Some(start) = t.find('<')
        && let Some(end) = t.rfind('>')
    {
        return t[start + 1..end].to_string();
    }
    t.to_string()
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
