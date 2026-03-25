pub mod context;
pub mod extractors;
pub mod parser;

use context::{AnalysisContext, ParsedFile};
use pecto_core::model::ProjectSpec;
use rayon::prelude::*;
use std::path::Path;

/// Analyze a C#/.NET project directory and extract behavior specs.
pub fn analyze_project(path: &Path) -> Result<ProjectSpec, CSharpAnalysisError> {
    let project_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    // Collect all C# source files
    let cs_files: Vec<walkdir::DirEntry> = walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension().is_some_and(|ext| ext == "cs")
                && !e.path().to_string_lossy().contains("/obj/")
                && !e.path().to_string_lossy().contains("/bin/")
                && !e.path().to_string_lossy().contains("\\obj\\")
                && !e.path().to_string_lossy().contains("\\bin\\")
                && !e.path().to_string_lossy().contains("/test/")
                && !e.path().to_string_lossy().contains("/Test/")
                && !e.path().to_string_lossy().contains("/Tests/")
        })
        .collect();

    // Parse files in parallel
    let parsed_results: Vec<Result<ParsedFile, CSharpAnalysisError>> = cs_files
        .par_iter()
        .map(|entry| {
            let source = std::fs::read_to_string(entry.path()).map_err(|e| {
                CSharpAnalysisError::IoError(entry.path().to_string_lossy().to_string(), e)
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
    let _ctx = AnalysisContext::new(parsed_files);

    let mut spec = ProjectSpec::new(project_name);
    spec.files_analyzed = files_analyzed;

    // Extractors will be added here as they are implemented:
    // for file in &ctx.files {
    //     - extractors::controller::extract(file, &ctx)
    //     - extractors::entity::extract(file, &ctx)
    //     - extractors::service::extract(file)
    //     - extractors::scheduled::extract(file)
    // }

    Ok(spec)
}

#[derive(Debug, thiserror::Error)]
pub enum CSharpAnalysisError {
    #[error("Failed to read {0}: {1}")]
    IoError(String, std::io::Error),
    #[error("Parse error in {0}: {1}")]
    ParseError(String, String),
}
