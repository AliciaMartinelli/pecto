use tree_sitter::{Parser, Tree};

/// Create a tree-sitter parser for Java.
pub fn java_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_java::LANGUAGE.into())
        .expect("Failed to load Java grammar");
    parser
}

/// Parse Java source code into a tree-sitter AST.
pub fn parse_java(source: &str) -> Option<Tree> {
    let mut parser = java_parser();
    parser.parse(source, None)
}
