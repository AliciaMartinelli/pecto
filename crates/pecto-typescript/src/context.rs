use crate::TypeScriptAnalysisError;
use crate::parser::parse_typescript;
use tree_sitter::Tree;

/// A parsed TypeScript/JavaScript source file.
pub struct ParsedFile {
    pub path: String,
    pub source: String,
    pub tree: Tree,
}

impl ParsedFile {
    pub fn parse(source: String, path: String) -> Result<Self, TypeScriptAnalysisError> {
        let is_tsx = path.ends_with(".tsx") || path.ends_with(".jsx");
        let tree = parse_typescript(&source, is_tsx).ok_or_else(|| {
            TypeScriptAnalysisError::ParseError(
                path.clone(),
                "Failed to parse TypeScript".to_string(),
            )
        })?;
        Ok(Self { path, source, tree })
    }
}

/// Holds all parsed files for cross-file lookups.
pub struct AnalysisContext {
    pub files: Vec<ParsedFile>,
}

impl AnalysisContext {
    pub fn new(files: Vec<ParsedFile>) -> Self {
        Self { files }
    }
}
