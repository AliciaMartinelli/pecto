use anyhow::{Context, Result};
use pecto_core::model::ProjectSpec;
use std::path::Path;

/// Generate a self-contained HTML report with embedded dependency graph.
pub fn generate_report(spec: &ProjectSpec, output: &Path) -> Result<()> {
    let html = crate::dashboard::render_html(spec, false);
    std::fs::write(output, html)
        .with_context(|| format!("Failed to write report to {}", output.display()))?;
    Ok(())
}
