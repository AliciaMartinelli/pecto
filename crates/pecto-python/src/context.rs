use crate::PythonAnalysisError;
use crate::parser::parse_python;
use tree_sitter::Tree;

/// A parsed Python source file ready for extraction.
pub struct ParsedFile {
    pub path: String,
    pub source: String,
    pub tree: Tree,
}

impl ParsedFile {
    pub fn parse(source: String, path: String) -> Result<Self, PythonAnalysisError> {
        let tree = parse_python(&source).ok_or_else(|| {
            PythonAnalysisError::ParseError(path.clone(), "Failed to parse Python".to_string())
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

    /// Find a class definition by name across all parsed files.
    pub fn find_class_by_name(&self, class_name: &str) -> Option<&ParsedFile> {
        use crate::extractors::common::node_text;

        for file in &self.files {
            let root = file.tree.root_node();
            let source = file.source.as_bytes();
            for i in 0..root.named_child_count() {
                let node = root.named_child(i).unwrap();
                // Python classes can be top-level or inside decorated_definition
                let class_node = if node.kind() == "class_definition" {
                    Some(node)
                } else if node.kind() == "decorated_definition" {
                    node.named_children(&mut node.walk())
                        .find(|c| c.kind() == "class_definition")
                } else {
                    None
                };

                if let Some(cn) = class_node
                    && let Some(name) = cn.child_by_field_name("name")
                    && node_text(&name, source) == class_name
                {
                    return Some(file);
                }
            }
        }
        None
    }
}
