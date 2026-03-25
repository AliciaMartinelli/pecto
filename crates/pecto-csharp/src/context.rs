use crate::CSharpAnalysisError;
use crate::parser::parse_csharp;
use tree_sitter::Tree;

/// A parsed C# source file ready for extraction.
pub struct ParsedFile {
    pub path: String,
    pub source: String,
    pub tree: Tree,
}

impl ParsedFile {
    /// Parse a C# source string into a ParsedFile.
    pub fn parse(source: String, path: String) -> Result<Self, CSharpAnalysisError> {
        let tree = parse_csharp(&source).ok_or_else(|| {
            CSharpAnalysisError::ParseError(path.clone(), "Failed to parse C#".to_string())
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
    pub fn find_class_by_name(&self, class_name: &str) -> Option<&ParsedFile> {
        for file in &self.files {
            let root = file.tree.root_node();
            let source = file.source.as_bytes();
            // C# files may have namespace > class, so we need to search recursively
            if find_class_recursive(&root, source, class_name) {
                return Some(file);
            }
        }
        None
    }
}

fn find_class_recursive(node: &tree_sitter::Node, source: &[u8], class_name: &str) -> bool {
    use crate::extractors::common::get_class_name;

    if node.kind() == "class_declaration" {
        let name = get_class_name(node, source);
        if name == class_name {
            return true;
        }
    }

    for i in 0..node.named_child_count() {
        let child = node.named_child(i).unwrap();
        if find_class_recursive(&child, source, class_name) {
            return true;
        }
    }
    false
}
