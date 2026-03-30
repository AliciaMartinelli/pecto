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

    let mut spec = pecto_csharp::analyze_project(&project_dir).expect("Analysis should succeed");
    pecto_core::domains::cluster_domains(&mut spec);

    assert!(
        spec.files_analyzed > 0,
        "Should analyze at least some files"
    );
    assert!(
        !spec.capabilities.is_empty(),
        "Should find at least one capability"
    );

    // Should find EF Core entities (via DbContext DbSet<T>)
    let entity_caps: Vec<_> = spec
        .capabilities
        .iter()
        .filter(|c| !c.entities.is_empty())
        .collect();
    assert!(!entity_caps.is_empty(), "Should find EF Core entities");

    let all_entities: Vec<String> = entity_caps
        .iter()
        .flat_map(|c| c.entities.iter().map(|e| e.name.clone()))
        .collect();
    assert!(
        all_entities.len() >= 5,
        "Should find at least 5 entities, found {}: {:?}",
        all_entities.len(),
        all_entities
    );

    // Should find endpoints (controllers)
    let total_endpoints: usize = spec.capabilities.iter().map(|c| c.endpoints.len()).sum();
    assert!(
        total_endpoints >= 10,
        "Should find at least 10 endpoints, found {}",
        total_endpoints
    );

    // Should find services (at least 5)
    let service_caps: Vec<_> = spec
        .capabilities
        .iter()
        .filter(|c| !c.operations.is_empty())
        .collect();
    assert!(
        service_caps.len() >= 5,
        "Should find at least 5 service capabilities, found {}",
        service_caps.len()
    );

    // Should have dependencies resolved
    assert!(
        spec.dependencies.len() >= 10,
        "Should have at least 10 dependencies, found {}",
        spec.dependencies.len()
    );

    // Should have domains clustered
    assert!(!spec.domains.is_empty(), "Should have domain clusters");

    // Print summary
    eprintln!("\n=== eShopOnWeb Analysis ===");
    eprintln!("Files analyzed: {}", spec.files_analyzed);
    eprintln!("Capabilities: {}", spec.capabilities.len());
    eprintln!("Dependencies: {}", spec.dependencies.len());
    eprintln!("Domains: {}", spec.domains.len());

    for domain in &spec.domains {
        eprintln!("  Domain '{}': {:?}", domain.name, domain.capabilities);
    }

    for dep in &spec.dependencies {
        eprintln!("  {} → {} ({:?})", dep.from, dep.to, dep.kind);
    }

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
