//! Integration tests against real Java projects.
//! These tests clone repositories and run pecto analysis on them.
//! Run with: cargo test --test integration_petclinic -- --ignored

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
fn test_spring_petclinic_rest() {
    let fixtures = fixture_dir();
    let project_dir = fixtures.join("spring-petclinic-rest");

    clone_or_update(
        "https://github.com/spring-petclinic/spring-petclinic-rest.git",
        &project_dir,
    );

    let spec = pecto_java::analyze_project(&project_dir).expect("Analysis should succeed");

    assert!(
        spec.files_analyzed > 0,
        "Should analyze at least some files"
    );
    assert!(
        !spec.capabilities.is_empty(),
        "Should find at least one capability"
    );

    // Should find JPA entities (Pet, Owner, Vet, Visit, etc.)
    let entity_caps: Vec<_> = spec
        .capabilities
        .iter()
        .filter(|c| !c.entities.is_empty())
        .collect();
    assert!(
        entity_caps.len() >= 5,
        "Should find at least 5 entity capabilities, found {}",
        entity_caps.len()
    );

    // Check specific entities exist
    let entity_names: Vec<String> = entity_caps
        .iter()
        .flat_map(|c| c.entities.iter().map(|e| e.name.clone()))
        .collect();
    assert!(
        entity_names.iter().any(|n| n == "Owner"),
        "Should find Owner entity"
    );
    assert!(
        entity_names.iter().any(|n| n == "Pet"),
        "Should find Pet entity"
    );
    assert!(
        entity_names.iter().any(|n| n == "Vet"),
        "Should find Vet entity"
    );

    // Should find controllers (even if they use interface-based mappings)
    let controller_caps: Vec<_> = spec
        .capabilities
        .iter()
        .filter(|c| c.source.contains("Controller"))
        .collect();
    assert!(
        !controller_caps.is_empty(),
        "Should find controller classes"
    );

    // Should find repositories
    let repo_caps: Vec<_> = spec
        .capabilities
        .iter()
        .filter(|c| c.name.contains("repository"))
        .collect();
    assert!(!repo_caps.is_empty(), "Should find repository interfaces");

    // Should find services
    let service_caps: Vec<_> = spec
        .capabilities
        .iter()
        .filter(|c| c.name.contains("service"))
        .collect();
    assert!(!service_caps.is_empty(), "Should find service classes");

    // Print summary for manual inspection
    eprintln!("\n=== Spring PetClinic REST Analysis ===");
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
