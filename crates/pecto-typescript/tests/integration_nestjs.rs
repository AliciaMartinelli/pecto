//! Integration tests against real TypeScript projects.
//! Run with: cargo test --test integration_nestjs -- --ignored

use std::path::Path;

#[test]
#[ignore]
fn test_nestjs_typeorm_sample() {
    let project_dir = Path::new(env!("HOME")).join("Desktop/nest/sample/05-sql-typeorm");

    if !project_dir.exists() {
        eprintln!("Skipping: {} does not exist", project_dir.display());
        return;
    }

    let mut spec =
        pecto_typescript::analyze_project(&project_dir).expect("Analysis should succeed");
    pecto_core::domains::cluster_domains(&mut spec);

    assert!(
        spec.files_analyzed > 0,
        "Should analyze at least some files"
    );

    // === Endpoints ===
    let total_endpoints: usize = spec.capabilities.iter().map(|c| c.endpoints.len()).sum();
    assert_eq!(
        total_endpoints, 4,
        "Should find exactly 4 endpoints (GET, GET/:id, POST, DELETE)"
    );

    // Check HTTP methods
    let all_endpoints: Vec<_> = spec
        .capabilities
        .iter()
        .flat_map(|c| &c.endpoints)
        .collect();
    assert!(
        all_endpoints
            .iter()
            .any(|e| e.method == pecto_core::model::HttpMethod::Get && e.path == "/users")
    );
    assert!(
        all_endpoints
            .iter()
            .any(|e| e.method == pecto_core::model::HttpMethod::Post && e.path == "/users")
    );
    assert!(
        all_endpoints
            .iter()
            .any(|e| e.method == pecto_core::model::HttpMethod::Delete && e.path.contains("users"))
    );

    // === Entity ===
    let all_entities: Vec<String> = spec
        .capabilities
        .iter()
        .flat_map(|c| c.entities.iter().map(|e| e.name.clone()))
        .collect();
    assert!(
        all_entities.contains(&"User".to_string()),
        "Should find User entity, found: {:?}",
        all_entities
    );

    let user_entity = spec
        .capabilities
        .iter()
        .flat_map(|c| c.entities.iter())
        .find(|e| e.name == "User")
        .unwrap();
    assert_eq!(user_entity.fields.len(), 4, "User should have 4 fields");
    assert!(user_entity.fields.iter().any(|f| f.name == "id"));
    assert!(user_entity.fields.iter().any(|f| f.name == "firstName"));

    // === Service ===
    let service_caps: Vec<_> = spec
        .capabilities
        .iter()
        .filter(|c| !c.operations.is_empty())
        .collect();
    assert!(!service_caps.is_empty(), "Should find service capabilities");

    let op_names: Vec<String> = service_caps
        .iter()
        .flat_map(|c| c.operations.iter().map(|o| o.name.clone()))
        .collect();
    assert_eq!(op_names.len(), 4, "Should find exactly 4 operations");
    assert!(op_names.contains(&"create".to_string()));
    assert!(op_names.contains(&"findAll".to_string()));
    assert!(op_names.contains(&"findOne".to_string()));
    assert!(op_names.contains(&"remove".to_string()));

    // === Dependencies ===
    assert!(
        !spec.dependencies.is_empty(),
        "Should have dependencies resolved"
    );
    // UsersController -> UsersService
    assert!(
        spec.dependencies
            .iter()
            .any(|d| d.from.contains("users") && d.to.contains("service")),
        "Should have controller -> service dependency"
    );

    // === Domains ===
    assert!(!spec.domains.is_empty(), "Should have domain clusters");

    // === Flows ===
    assert!(!spec.flows.is_empty(), "Should have flow traces");

    // Should NOT include test files
    assert!(
        !spec
            .capabilities
            .iter()
            .any(|c| c.source.contains("test/") || c.source.contains(".spec.")),
        "Should not include test files"
    );

    // Print summary
    eprintln!("\n=== NestJS TypeORM Sample Analysis ===");
    eprintln!("Files: {}", spec.files_analyzed);
    eprintln!("Capabilities: {}", spec.capabilities.len());
    eprintln!("Endpoints: {}", total_endpoints);
    eprintln!("Dependencies: {}", spec.dependencies.len());
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
