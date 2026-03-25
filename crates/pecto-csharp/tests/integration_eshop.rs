//! Integration tests against real C# projects.
//! Run with: cargo test --test integration_eshop -- --ignored

use std::path::Path;
use std::process::Command;

fn clone_or_update(url: &str, target: &Path) {
    if target.exists() {
        let _ = Command::new("git")
            .args(["pull", "--quiet"])
            .current_dir(target)
            .status();
    } else {
        let status = Command::new("git")
            .args(["clone", "--depth", "1", "--quiet", url])
            .arg(target)
            .status()
            .expect("Failed to run git clone");
        assert!(status.success(), "git clone failed for {}", url);
    }
}

fn fixture_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("pecto-test-fixtures");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
#[ignore]
fn test_eshop_on_web() {
    let fixtures = fixture_dir();
    let project_dir = fixtures.join("eShopOnWeb");

    clone_or_update(
        "https://github.com/dotnet-architecture/eShopOnWeb.git",
        &project_dir,
    );

    let spec = pecto_csharp::analyze_project(&project_dir).expect("Analysis should succeed");

    assert!(
        spec.files_analyzed > 0,
        "Should analyze at least some files"
    );
    assert!(
        !spec.capabilities.is_empty(),
        "Should find at least one capability"
    );

    // Should find entities (either via [Table]/[Key] or DbContext)
    let entity_caps: Vec<_> = spec
        .capabilities
        .iter()
        .filter(|c| !c.entities.is_empty())
        .collect();
    assert!(!entity_caps.is_empty(), "Should find EF Core entities");

    // Should find services
    let service_caps: Vec<_> = spec
        .capabilities
        .iter()
        .filter(|c| c.name.contains("service"))
        .collect();
    assert!(!service_caps.is_empty(), "Should find service classes");

    // Print summary
    eprintln!("\n=== eShopOnWeb Analysis ===");
    eprintln!("Files analyzed: {}", spec.files_analyzed);
    eprintln!("Capabilities: {}", spec.capabilities.len());
    for cap in &spec.capabilities {
        let detail = if !cap.endpoints.is_empty() {
            format!("{} endpoints", cap.endpoints.len())
        } else if !cap.entities.is_empty() {
            format!("{} entities", cap.entities.len())
        } else if !cap.operations.is_empty() {
            format!("{} operations", cap.operations.len())
        } else if !cap.scheduled_tasks.is_empty() {
            format!("{} tasks", cap.scheduled_tasks.len())
        } else {
            "empty".to_string()
        };
        eprintln!("  - {} ({})", cap.name, detail);
    }
}
