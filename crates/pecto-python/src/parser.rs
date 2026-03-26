use tree_sitter::{Parser, Tree};

/// Create a tree-sitter parser for Python.
pub fn python_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .expect("Failed to load Python grammar");
    parser
}

/// Parse Python source code into a tree-sitter AST.
pub fn parse_python(source: &str) -> Option<Tree> {
    let mut parser = python_parser();
    parser.parse(source, None)
}
