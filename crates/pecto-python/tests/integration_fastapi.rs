//! Integration tests against real Python projects.
//! Run with: cargo test --test integration_fastapi -- --ignored

use std::path::Path;

#[test]
#[ignore]
fn test_fastapi_template() {
    let project_dir = Path::new(env!("HOME"))
        .join("Desktop/full-stack-fastapi-template/backend");

    if !project_dir.exists() {
        eprintln!(
            "Skipping: {} does not exist",
            project_dir.display()
        );
        return;
    }

    let mut spec =
        pecto_python::analyze_project(&project_dir).expect("Analysis should succeed");
    pecto_core::domains::cluster_domains(&mut spec);

    assert!(
        spec.files_analyzed > 0,
        "Should analyze at least some files"
    );
    assert!(
        !spec.capabilities.is_empty(),
        "Should find capabilities"
    );

    // === Endpoints ===
    let total_endpoints: usize = spec
        .capabilities
        .iter()
        .map(|c| c.endpoints.len())
        .sum();
    assert!(
        total_endpoints >= 20,
        "Should find at least 20 endpoints, found {}",
        total_endpoints
    );

    // Check specific route files are detected
    let endpoint_caps: Vec<_> = spec
        .capabilities
        .iter()
        .filter(|c| !c.endpoints.is_empty())
        .collect();
    let cap_names: Vec<&str> = endpoint_caps.iter().map(|c| c.name.as_str()).collect();
    assert!(cap_names.contains(&"items"), "Should find items routes");
    assert!(cap_names.contains(&"users"), "Should find users routes");
    assert!(cap_names.contains(&"login"), "Should find login routes");

    // === Entities (SQLModel table=True) ===
    let all_entities: Vec<String> = spec
        .capabilities
        .iter()
        .flat_map(|c| c.entities.iter().map(|e| e.name.clone()))
        .collect();
    assert!(
        all_entities.iter().any(|n| n == "User"),
        "Should find User entity (SQLModel table=True), found: {:?}",
        all_entities
    );
    assert!(
        all_entities.iter().any(|n| n == "Item"),
        "Should find Item entity (SQLModel table=True), found: {:?}",
        all_entities
    );

    // User should have fields
    let user_entity = spec
        .capabilities
        .iter()
        .flat_map(|c| c.entities.iter())
        .find(|e| e.name == "User")
        .expect("User entity should exist");
    assert!(
        !user_entity.fields.is_empty(),
        "User entity should have fields"
    );
    assert!(
        user_entity.fields.iter().any(|f| f.name == "id"),
        "User should have id field"
    );

    // === Pydantic DTOs ===
    assert!(
        all_entities.iter().any(|n| n == "UserCreate" || n == "UserBase"),
        "Should find Pydantic DTO models, found: {:?}",
        all_entities
    );

    // === Services (module-level CRUD) ===
    let service_caps: Vec<_> = spec
        .capabilities
        .iter()
        .filter(|c| !c.operations.is_empty())
        .collect();
    assert!(
        !service_caps.is_empty(),
        "Should find service capabilities (crud.py)"
    );
    let crud_ops: Vec<String> = service_caps
        .iter()
        .flat_map(|c| c.operations.iter().map(|o| o.name.clone()))
        .collect();
    assert!(
        crud_ops.iter().any(|n| n == "create_user"),
        "Should find create_user operation, found: {:?}",
        crud_ops
    );

    // === Dependencies ===
    assert!(
        !spec.dependencies.is_empty(),
        "Should have dependencies resolved"
    );

    // === Domains ===
    assert!(!spec.domains.is_empty(), "Should have domain clusters");

    // === Flows ===
    assert!(!spec.flows.is_empty(), "Should have flow traces");

    // Print summary for manual inspection
    eprintln!("\n=== FastAPI Template Analysis ===");
    eprintln!("Files analyzed: {}", spec.files_analyzed);
    eprintln!("Capabilities: {}", spec.capabilities.len());
    eprintln!("Total endpoints: {}", total_endpoints);
    eprintln!("Total entities: {}", all_entities.len());
    eprintln!("Dependencies: {}", spec.dependencies.len());
    eprintln!("Domains: {}", spec.domains.len());
    eprintln!("Flows: {}", spec.flows.len());
    for cap in &spec.capabilities {
        let detail = if !cap.endpoints.is_empty() {
            format!("{} endpoints", cap.endpoints.len())
        } else if !cap.entities.is_empty() {
            format!("{} entities", cap.entities.len())
        } else if !cap.operations.is_empty() {
            format!("{} operations", cap.operations.len())
        } else {
            "empty".to_string()
        };
        eprintln!("  - {} ({})", cap.name, detail);
    }
}
