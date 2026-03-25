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

    /// Find a class declaration by its simple name across all parsed files.
    /// Returns the source bytes and the class_declaration node's file index.
    pub fn find_class_by_name(&self, class_name: &str) -> Option<&ParsedFile> {
        use crate::extractors::common::get_class_name;

        for file in &self.files {
            let root = file.tree.root_node();
            let source = file.source.as_bytes();
            for i in 0..root.named_child_count() {
                let node = root.named_child(i).unwrap();
                if node.kind() == "class_declaration" {
                    let name = get_class_name(&node, source);
                    if name == class_name {
                        return Some(file);
                    }
                }
                // Also check for class inside a package declaration wrapper
                // (tree-sitter sometimes wraps in program > class_declaration)
            }
        }
        None
    }
}
