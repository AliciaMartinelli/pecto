pub mod context;
pub mod dependencies;
pub mod extractors;
pub mod flow;
pub mod parser;

use context::{AnalysisContext, ParsedFile};
use pecto_core::model::ProjectSpec;
use rayon::prelude::*;
use std::path::Path;

/// Analyze a Python project directory and extract behavior specs.
pub fn analyze_project(path: &Path) -> Result<ProjectSpec, PythonAnalysisError> {
    let project_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let py_files: Vec<walkdir::DirEntry> = walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let p = e.path().to_string_lossy();
            e.path().extension().is_some_and(|ext| ext == "py")
                && !p.contains("__pycache__")
                && !p.contains("/venv/")
                && !p.contains("/.venv/")
                && !p.contains("/env/")
                && !p.contains("/node_modules/")
                && !p.contains("/migrations/")
                && !p.contains("/test/")
                && !p.contains("/tests/")
        })
        .collect();

    let parsed_results: Vec<Result<ParsedFile, PythonAnalysisError>> = py_files
        .par_iter()
        .map(|entry| {
            let source = std::fs::read_to_string(entry.path()).map_err(|e| {
                PythonAnalysisError::IoError(entry.path().to_string_lossy().to_string(), e)
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

    let mut parsed_files = Vec::with_capacity(parsed_results.len());
    for result in parsed_results {
        parsed_files.push(result?);
    }

    let files_analyzed = parsed_files.len();
    let ctx = AnalysisContext::new(parsed_files);

    let mut spec = ProjectSpec::new(project_name);
    spec.files_analyzed = files_analyzed;

    for file in &ctx.files {
        if let Some(capability) = extractors::controller::extract(file)
            && !capability.is_empty()
        {
            spec.capabilities.push(capability);
        }

        if let Some(capability) = extractors::entity::extract(file)
            && !capability.is_empty()
        {
            spec.capabilities.push(capability);
        }

        if let Some(capability) = extractors::service::extract(file)
            && !capability.is_empty()
        {
            spec.capabilities.push(capability);
        }

        if let Some(capability) = extractors::scheduled::extract(file)
            && !capability.is_empty()
        {
            spec.capabilities.push(capability);
        }
    }

    dependencies::resolve_dependencies(&mut spec, &ctx);
    flow::extract_flows(&mut spec, &ctx);

    Ok(spec)
}

#[derive(Debug, thiserror::Error)]
pub enum PythonAnalysisError {
    #[error("Failed to read {0}: {1}")]
    IoError(String, std::io::Error),
    #[error("Parse error in {0}: {1}")]
    ParseError(String, String),
}
