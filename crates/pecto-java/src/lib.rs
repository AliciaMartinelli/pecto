pub mod parser;
pub mod spring;

use pecto_core::model::ProjectSpec;
use std::path::Path;

/// Analyze a Java project directory and extract behavior specs.
pub fn analyze_project(path: &Path) -> Result<ProjectSpec, JavaAnalysisError> {
    let project_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let mut spec = ProjectSpec::new(project_name);
    let mut files_analyzed = 0usize;

    let java_files: Vec<walkdir::DirEntry> = walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(|e: Result<walkdir::DirEntry, walkdir::Error>| e.ok())
        .filter(|e: &walkdir::DirEntry| {
            e.path().extension().is_some_and(|ext| ext == "java")
                && !e.path().to_string_lossy().contains("/test/")
        })
        .collect();

    for entry in &java_files {
        let source = std::fs::read_to_string(entry.path()).map_err(|e| {
            JavaAnalysisError::IoError(entry.path().to_string_lossy().to_string(), e)
        })?;

        let relative_path = entry
            .path()
            .strip_prefix(path)
            .unwrap_or(entry.path())
            .to_string_lossy()
            .to_string();

        if let Some(capability) = spring::extract_capability(&source, &relative_path)?
            && !capability.is_empty() {
                spec.capabilities.push(capability);
            }

        files_analyzed += 1;
    }

    spec.files_analyzed = files_analyzed;
    Ok(spec)
}

#[derive(Debug, thiserror::Error)]
pub enum JavaAnalysisError {
    #[error("Failed to read {0}: {1}")]
    IoError(String, std::io::Error),
    #[error("Parse error in {0}: {1}")]
    ParseError(String, String),
}
