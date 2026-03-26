use tree_sitter::Node;

/// Parsed decorator info for TypeScript/NestJS `@Decorator(args)`.
#[derive(Debug)]
pub struct DecoratorInfo {
    pub name: String,
    pub args: Vec<String>,
}

/// Collect decorators from a class or method (TypeScript experimental decorators).
pub fn collect_decorators(node: &Node, source: &[u8]) -> Vec<DecoratorInfo> {
    let mut decorators = Vec::new();

    for i in 0..node.named_child_count() {
        let child = node.named_child(i).unwrap();
        if child.kind() == "decorator"
            && let Some(info) = parse_decorator(&child, source)
        {
            decorators.push(info);
        }
    }

    decorators
}

fn parse_decorator(node: &Node, source: &[u8]) -> Option<DecoratorInfo> {
    // Decorator: @ expression
    // expression can be: identifier, call_expression
    for i in 0..node.named_child_count() {
        let child = node.named_child(i).unwrap();
        match child.kind() {
            "identifier" => {
                return Some(DecoratorInfo {
                    name: node_text(&child, source),
                    args: Vec::new(),
                });
            }
            "call_expression" => {
                let name = child
                    .child_by_field_name("function")
                    .map(|f| node_text(&f, source))
                    .unwrap_or_default();
                let mut args = Vec::new();
                if let Some(arg_list) = child.child_by_field_name("arguments") {
                    for j in 0..arg_list.named_child_count() {
                        let arg = arg_list.named_child(j).unwrap();
                        args.push(node_text(&arg, source));
                    }
                }
                return Some(DecoratorInfo { name, args });
            }
            _ => {}
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
    s.trim_matches('"')
        .trim_matches('\'')
        .trim_matches('`')
        .to_string()
}

/// Convert to kebab-case.
pub fn to_kebab_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c == '_' || c == '.' {
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

/// Get the name from a class/function declaration.
pub fn get_def_name(node: &Node, source: &[u8]) -> String {
    node.child_by_field_name("name")
        .map(|n| node_text(&n, source))
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ParsedFile;

    fn parse_file(source: &str, path: &str) -> ParsedFile {
        ParsedFile::parse(source.to_string(), path.to_string()).unwrap()
    }

    #[test]
    fn test_parse_nestjs_decorators() {
        let source = r#"
import { Controller, Get, Post } from '@nestjs/common';

@Controller('users')
export class UsersController {
    @Get()
    findAll() {
        return [];
    }

    @Post()
    create(@Body() dto: CreateUserDto) {
        return dto;
    }
}
"#;

        let file = parse_file(source, "users.controller.ts");
        let root = file.tree.root_node();
        let src = file.source.as_bytes();

        // Collect all decorators found anywhere in the AST
        let mut all_decorators = Vec::new();
        fn find_decorators_recursive(
            node: &tree_sitter::Node,
            source: &[u8],
            results: &mut Vec<String>,
        ) {
            let decs = super::collect_decorators(node, source);
            for d in &decs {
                results.push(d.name.clone());
            }
            for i in 0..node.named_child_count() {
                let child = node.named_child(i).unwrap();
                find_decorators_recursive(&child, source, results);
            }
        }
        find_decorators_recursive(&root, src, &mut all_decorators);

        assert!(
            all_decorators.iter().any(|d| d == "Controller"),
            "Should find @Controller decorator somewhere in AST. Found: {:?}",
            all_decorators
        );
    }

    #[test]
    fn test_clean_string_literal() {
        assert_eq!(clean_string_literal("\"hello\""), "hello");
        assert_eq!(clean_string_literal("'/api'"), "/api");
        assert_eq!(clean_string_literal("`template`"), "template");
    }

    #[test]
    fn test_to_kebab_case() {
        assert_eq!(to_kebab_case("UsersController"), "users-controller");
        assert_eq!(to_kebab_case("user_service"), "user-service");
    }
}
