use anyhow::{Context, Result};
use axum::{Json, Router, extract::State, response::Html, routing::get};
use pecto_core::model::ProjectSpec;
use std::sync::Arc;

/// Start a local web server serving the interactive pecto dashboard.
pub fn serve(spec: ProjectSpec, port: u16) -> Result<()> {
    let rt = tokio::runtime::Runtime::new().context("Failed to create tokio runtime")?;
    rt.block_on(async {
        let state = Arc::new(spec);

        let app = Router::new()
            .route("/", get(index_handler))
            .route("/api/spec", get(spec_handler))
            .with_state(state);

        let addr = format!("0.0.0.0:{}", port);
        let listener = tokio::net::TcpListener::bind(&addr)
            .await
            .with_context(|| format!("Failed to bind to {}", addr))?;

        eprintln!(
            "  Dashboard: http://localhost:{}\n  Press Ctrl+C to stop\n",
            port
        );

        axum::serve(listener, app).await.context("Server error")?;

        Ok(())
    })
}

async fn spec_handler(State(spec): State<Arc<ProjectSpec>>) -> Json<ProjectSpec> {
    Json((*spec).clone())
}

async fn index_handler(State(spec): State<Arc<ProjectSpec>>) -> Html<String> {
    Html(crate::dashboard::render_html(&spec, true))
}
