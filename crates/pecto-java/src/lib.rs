pub mod context;
pub mod dependencies;
pub mod extractors;
pub mod parser;

use context::{AnalysisContext, ParsedFile};
use pecto_core::model::ProjectSpec;
use rayon::prelude::*;
use std::path::Path;

/// Analyze a Java project directory and extract behavior specs.
pub fn analyze_project(path: &Path) -> Result<ProjectSpec, JavaAnalysisError> {
    let project_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    // Collect all Java source file paths
    let java_files: Vec<walkdir::DirEntry> = walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension().is_some_and(|ext| ext == "java")
                && !e.path().to_string_lossy().contains("/test/")
        })
        .collect();

    // Read and parse files in parallel using rayon
    let parsed_results: Vec<Result<ParsedFile, JavaAnalysisError>> = java_files
        .par_iter()
        .map(|entry| {
            let source = std::fs::read_to_string(entry.path()).map_err(|e| {
                JavaAnalysisError::IoError(entry.path().to_string_lossy().to_string(), e)
            })?;

            let relative_path = entry
                .path()
                .strip_prefix(path)
                .unwrap_or(entry.path())
                .to_string_lossy()
                .to_string();

            ParsedFile::parse(source, relative_path)
        })
        .collect();

    // Collect results, propagating errors
    let mut parsed_files = Vec::with_capacity(parsed_results.len());
    for result in parsed_results {
        parsed_files.push(result?);
    }

    let files_analyzed = parsed_files.len();
    let ctx = AnalysisContext::new(parsed_files);

    // Run all extractors (sequential — they may share cross-file context)
    let mut spec = ProjectSpec::new(project_name);
    spec.files_analyzed = files_analyzed;

    for file in &ctx.files {
        // Controller extraction
        if let Some(capability) = extractors::controller::extract(file, &ctx)
            && !capability.is_empty()
        {
            spec.capabilities.push(capability);
        }

        // Entity extraction
        if let Some(capability) = extractors::entity::extract(file)
            && !capability.is_empty()
        {
            spec.capabilities.push(capability);
        }

        // Repository extraction
        if let Some(capability) = extractors::repository::extract(file)
            && !capability.is_empty()
        {
            spec.capabilities.push(capability);
        }

        // Service extraction
        if let Some(capability) = extractors::service::extract(file)
            && !capability.is_empty()
        {
            spec.capabilities.push(capability);
        }

        // Scheduled tasks + event listeners
        if let Some(capability) = extractors::scheduled::extract(file)
            && !capability.is_empty()
        {
            spec.capabilities.push(capability);
        }
    }

    // Post-processing: resolve dependencies between capabilities
    dependencies::resolve_dependencies(&mut spec, &ctx);

    Ok(spec)
}

#[derive(Debug, thiserror::Error)]
pub enum JavaAnalysisError {
    #[error("Failed to read {0}: {1}")]
    IoError(String, std::io::Error),
    #[error("Parse error in {0}: {1}")]
    ParseError(String, String),
}
