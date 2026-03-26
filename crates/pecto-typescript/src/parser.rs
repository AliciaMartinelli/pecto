use tree_sitter::{Parser, Tree};

/// Create a tree-sitter parser for TypeScript.
pub fn typescript_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
        .expect("Failed to load TypeScript grammar");
    parser
}

/// Create a tree-sitter parser for TSX.
pub fn tsx_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TSX.into())
        .expect("Failed to load TSX grammar");
    parser
}

/// Parse TypeScript/JavaScript source code into a tree-sitter AST.
pub fn parse_typescript(source: &str, is_tsx: bool) -> Option<Tree> {
    let mut parser = if is_tsx {
        tsx_parser()
    } else {
        typescript_parser()
    };
    parser.parse(source, None)
}
