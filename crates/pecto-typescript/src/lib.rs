pub mod context;
pub mod dependencies;
pub mod extractors;
pub mod flow;
pub mod parser;

use context::{AnalysisContext, ParsedFile};
use pecto_core::model::ProjectSpec;
use rayon::prelude::*;
use std::path::Path;

/// Analyze a TypeScript/JavaScript project and extract behavior specs.
pub fn analyze_project(path: &Path) -> Result<ProjectSpec, TypeScriptAnalysisError> {
    let project_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let ts_files: Vec<walkdir::DirEntry> = walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let p = e.path().to_string_lossy();
            let ext_ok = e
                .path()
                .extension()
                .is_some_and(|ext| ext == "ts" || ext == "tsx" || ext == "js" || ext == "jsx");
            ext_ok
                && !p.contains("node_modules")
                && !p.contains("/dist/")
                && !p.contains("/build/")
                && !p.contains("/.next/")
                && !p.contains("/__tests__/")
                && !p.contains("/__mocks__/")
                && !p.ends_with(".spec.ts")
                && !p.ends_with(".test.ts")
                && !p.ends_with(".spec.js")
                && !p.ends_with(".test.js")
                && !p.ends_with(".d.ts")
        })
        .collect();

    let parsed_results: Vec<Result<ParsedFile, TypeScriptAnalysisError>> = ts_files
        .par_iter()
        .map(|entry| {
            let source = std::fs::read_to_string(entry.path()).map_err(|e| {
                TypeScriptAnalysisError::IoError(entry.path().to_string_lossy().to_string(), e)
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
    }

    // Post-processing
    dependencies::resolve_dependencies(&mut spec, &ctx);
    flow::extract_flows(&mut spec, &ctx);

    Ok(spec)
}

#[derive(Debug, thiserror::Error)]
pub enum TypeScriptAnalysisError {
    #[error("Failed to read {0}: {1}")]
    IoError(String, std::io::Error),
    #[error("Parse error in {0}: {1}")]
    ParseError(String, String),
}
