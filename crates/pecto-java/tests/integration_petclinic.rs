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

    let mut spec = pecto_java::analyze_project(&project_dir).expect("Analysis should succeed");
    pecto_core::domains::cluster_domains(&mut spec);

    assert!(
        spec.files_analyzed > 0,
        "Should analyze at least some files"
    );
    assert!(
        !spec.capabilities.is_empty(),
        "Should find at least one capability"
    );

    // Should find all 8 JPA entities
    let entity_caps: Vec<_> = spec
        .capabilities
        .iter()
        .filter(|c| !c.entities.is_empty())
        .collect();
    assert!(
        entity_caps.len() >= 7,
        "Should find at least 7 entity capabilities, found {}",
        entity_caps.len()
    );

    // Check all expected entities exist
    let entity_names: Vec<String> = entity_caps
        .iter()
        .flat_map(|c| c.entities.iter().map(|e| e.name.clone()))
        .collect();
    for expected in &["Owner", "Pet", "Vet", "Visit", "Specialty", "PetType", "Role", "User"] {
        assert!(
            entity_names.iter().any(|n| n == expected),
            "Should find {} entity, found: {:?}",
            expected,
            entity_names
        );
    }

    // Most entities should have fields (some like Specialty inherit all fields from parent)
    let entities_with_fields = entity_caps
        .iter()
        .flat_map(|c| c.entities.iter())
        .filter(|e| !e.fields.is_empty())
        .count();
    assert!(
        entities_with_fields >= 5,
        "At least 5 entities should have direct fields, found {}",
        entities_with_fields
    );

    // NOTE: PetClinic v4 uses OpenAPI-generated interfaces for endpoint annotations.
    // Only RootRestController has direct @RequestMapping on methods.
    // Other controllers implement generated API interfaces whose source is not in the tree.
    let endpoint_count: usize = spec.capabilities.iter().map(|c| c.endpoints.len()).sum();
    assert!(
        endpoint_count >= 1,
        "Should find at least 1 endpoint (RootRestController)"
    );

    // Should find repositories (at least 7: Owner, Pet, Vet, Visit, PetType, Specialty, User)
    let repo_caps: Vec<_> = spec
        .capabilities
        .iter()
        .filter(|c| c.name.contains("repository"))
        .collect();
    assert!(
        repo_caps.len() >= 7,
        "Should find at least 7 repository capabilities, found {}",
        repo_caps.len()
    );

    // Should find services (ClinicService, UserService)
    let service_caps: Vec<_> = spec
        .capabilities
        .iter()
        .filter(|c| c.name.contains("service"))
        .collect();
    assert!(
        service_caps.len() >= 2,
        "Should find at least 2 service capabilities, found {}",
        service_caps.len()
    );

    // Should have dependencies resolved (service → repository)
    assert!(
        spec.dependencies.len() >= 5,
        "Should have at least 5 dependencies, found {}",
        spec.dependencies.len()
    );
    eprintln!("\n=== Dependencies ===");
    for dep in &spec.dependencies {
        eprintln!("  {} → {} ({:?})", dep.from, dep.to, dep.kind);
    }

    // Should have domains clustered
    assert!(!spec.domains.is_empty(), "Should have domain clusters");
    eprintln!("\n=== Domains ===");
    for domain in &spec.domains {
        eprintln!(
            "  {} ({} caps): {:?}",
            domain.name,
            domain.capabilities.len(),
            domain.capabilities
        );
    }

    // Print summary for manual inspection
    eprintln!("\n=== Spring PetClinic REST Analysis ===");
    eprintln!("Files analyzed: {}", spec.files_analyzed);
    eprintln!("Capabilities: {}", spec.capabilities.len());
    eprintln!("Dependencies: {}", spec.dependencies.len());
    eprintln!("Domains: {}", spec.domains.len());
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
