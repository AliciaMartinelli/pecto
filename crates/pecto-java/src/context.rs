use crate::JavaAnalysisError;
use crate::parser::parse_java;
use tree_sitter::Tree;

/// A parsed Java source file ready for extraction.
pub struct ParsedFile {
    pub path: String,
    pub source: String,
    pub tree: Tree,
}

impl ParsedFile {
    /// Parse a Java source string into a ParsedFile.
    pub fn parse(source: String, path: String) -> Result<Self, JavaAnalysisError> {
        let tree = parse_java(&source).ok_or_else(|| {
            JavaAnalysisError::ParseError(path.clone(), "Failed to parse Java".to_string())
        })?;
        Ok(Self { path, source, tree })
    }
}

/// Holds all parsed files for cross-file lookups during extraction.
pub struct AnalysisContext {
    pub files: Vec<ParsedFile>,
}

impl AnalysisContext {
    pub fn new(files: Vec<ParsedFile>) -> Self {
        Self { files }
    }
}
