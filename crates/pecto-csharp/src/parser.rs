use tree_sitter::{Parser, Tree};

/// Create a tree-sitter parser for C#.
pub fn csharp_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_c_sharp::LANGUAGE.into())
        .expect("Failed to load C# grammar");
    parser
}

/// Parse C# source code into a tree-sitter AST.
pub fn parse_csharp(source: &str) -> Option<Tree> {
    let mut parser = csharp_parser();
    parser.parse(source, None)
}
