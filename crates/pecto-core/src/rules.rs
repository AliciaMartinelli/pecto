use crate::model::*;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// Configuration for architecture fitness rules.
#[derive(Debug, Deserialize)]
pub struct RulesConfig {
    #[serde(default = "default_rules")]
    pub rules: BTreeMap<String, RuleValue>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum RuleValue {
    Enabled(bool),
    Threshold(u32),
}

impl RuleValue {
    fn is_enabled(&self) -> bool {
        match self {
            RuleValue::Enabled(b) => *b,
            RuleValue::Threshold(_) => true,
        }
    }

    fn threshold(&self) -> u32 {
        match self {
            RuleValue::Threshold(n) => *n,
            _ => 0,
        }
    }
}

fn default_rules() -> BTreeMap<String, RuleValue> {
    let mut rules = BTreeMap::new();
    rules.insert(
        "no-circular-dependencies".to_string(),
        RuleValue::Enabled(true),
    );
    rules.insert(
        "controllers-no-direct-db-access".to_string(),
        RuleValue::Enabled(true),
    );
    rules.insert(
        "all-endpoints-need-authentication".to_string(),
        RuleValue::Enabled(true),
    );
    rules.insert(
        "no-entity-without-validation".to_string(),
        RuleValue::Enabled(false),
    );
    rules.insert(
        "max-service-dependencies".to_string(),
        RuleValue::Threshold(5),
    );
    rules
}

impl Default for RulesConfig {
    fn default() -> Self {
        RulesConfig {
            rules: default_rules(),
        }
    }
}

/// Result of checking a single rule.
#[derive(Debug)]
pub struct RuleResult {
    pub name: String,
    pub passed: bool,
    pub violations: Vec<String>,
}

/// Check all enabled rules against the given spec.
pub fn check_rules(spec: &ProjectSpec, config: &RulesConfig) -> Vec<RuleResult> {
    let mut results = Vec::new();

    for (name, value) in &config.rules {
        if !value.is_enabled() {
            continue;
        }

        let result = match name.as_str() {
            "no-circular-dependencies" => check_no_circular_deps(spec),
            "controllers-no-direct-db-access" => check_controllers_no_db(spec),
            "all-endpoints-need-authentication" => check_all_endpoints_auth(spec),
            "no-entity-without-validation" => check_entity_validation(spec),
            "max-service-dependencies" => check_max_service_deps(spec, value.threshold()),
            _ => RuleResult {
                name: name.clone(),
                passed: true,
                violations: vec![format!("Unknown rule: {}", name)],
            },
        };

        results.push(result);
    }

    results
}

// ─── Rule implementations ───

/// No circular dependencies in the dependency graph.
fn check_no_circular_deps(spec: &ProjectSpec) -> RuleResult {
    let mut violations = Vec::new();

    // Build adjacency list
    let mut graph: HashMap<&str, Vec<&str>> = HashMap::new();
    for dep in &spec.dependencies {
        graph
            .entry(dep.from.as_str())
            .or_default()
            .push(dep.to.as_str());
    }

    // DFS cycle detection
    let mut visited = HashSet::new();
    let mut in_stack = HashSet::new();

    for node in graph.keys() {
        if !visited.contains(node) {
            let mut path = Vec::new();
            if has_cycle(node, &graph, &mut visited, &mut in_stack, &mut path) {
                violations.push(format!("Cycle: {}", path.join(" → ")));
            }
        }
    }

    RuleResult {
        name: "no-circular-dependencies".to_string(),
        passed: violations.is_empty(),
        violations,
    }
}

fn has_cycle<'a>(
    node: &'a str,
    graph: &HashMap<&'a str, Vec<&'a str>>,
    visited: &mut HashSet<&'a str>,
    in_stack: &mut HashSet<&'a str>,
    path: &mut Vec<String>,
) -> bool {
    visited.insert(node);
    in_stack.insert(node);
    path.push(node.to_string());

    if let Some(neighbors) = graph.get(node) {
        for &neighbor in neighbors {
            if !visited.contains(neighbor) {
                if has_cycle(neighbor, graph, visited, in_stack, path) {
                    return true;
                }
            } else if in_stack.contains(neighbor) {
                path.push(neighbor.to_string());
                return true;
            }
        }
    }

    in_stack.remove(node);
    path.pop();
    false
}

/// Controllers should not directly depend on entities/repositories (should go through services).
fn check_controllers_no_db(spec: &ProjectSpec) -> RuleResult {
    let mut violations = Vec::new();

    // Identify controller and entity/repository capabilities
    let controllers: BTreeSet<&str> = spec
        .capabilities
        .iter()
        .filter(|c| !c.endpoints.is_empty())
        .map(|c| c.name.as_str())
        .collect();

    let db_caps: BTreeSet<&str> = spec
        .capabilities
        .iter()
        .filter(|c| {
            !c.entities.is_empty() || c.name.contains("repository") || c.name.contains("context")
        })
        .map(|c| c.name.as_str())
        .collect();

    for dep in &spec.dependencies {
        if controllers.contains(dep.from.as_str()) && db_caps.contains(dep.to.as_str()) {
            violations.push(format!(
                "{} → {} (controller directly accesses data layer)",
                dep.from, dep.to
            ));
        }
    }

    RuleResult {
        name: "controllers-no-direct-db-access".to_string(),
        passed: violations.is_empty(),
        violations,
    }
}

/// All endpoints must have authentication configured.
fn check_all_endpoints_auth(spec: &ProjectSpec) -> RuleResult {
    let mut violations = Vec::new();

    for cap in &spec.capabilities {
        for ep in &cap.endpoints {
            let has_auth = ep
                .security
                .as_ref()
                .is_some_and(|s| s.authentication.is_some());
            if !has_auth {
                let method = format!("{:?}", ep.method).to_uppercase();
                violations.push(format!(
                    "{} {} ({}) — no authentication",
                    method, ep.path, cap.name
                ));
            }
        }
    }

    RuleResult {
        name: "all-endpoints-need-authentication".to_string(),
        passed: violations.is_empty(),
        violations,
    }
}

/// Endpoints with request body input should have validation rules.
fn check_entity_validation(spec: &ProjectSpec) -> RuleResult {
    let mut violations = Vec::new();

    for cap in &spec.capabilities {
        for ep in &cap.endpoints {
            let has_body = ep.input.as_ref().is_some_and(|i| i.body.is_some());
            let has_validation = !ep.validation.is_empty();

            if has_body && !has_validation {
                let method = format!("{:?}", ep.method).to_uppercase();
                violations.push(format!(
                    "{} {} ({}) — has request body but no validation",
                    method, ep.path, cap.name
                ));
            }
        }
    }

    RuleResult {
        name: "no-entity-without-validation".to_string(),
        passed: violations.is_empty(),
        violations,
    }
}

/// No service should have more than N outgoing dependencies.
fn check_max_service_deps(spec: &ProjectSpec, max: u32) -> RuleResult {
    let mut violations = Vec::new();

    // Count outgoing deps per capability
    let mut dep_count: HashMap<&str, u32> = HashMap::new();
    for dep in &spec.dependencies {
        *dep_count.entry(dep.from.as_str()).or_default() += 1;
    }

    // Only check service capabilities
    let services: BTreeSet<&str> = spec
        .capabilities
        .iter()
        .filter(|c| !c.operations.is_empty())
        .map(|c| c.name.as_str())
        .collect();

    for (name, count) in &dep_count {
        if services.contains(name) && *count > max {
            violations.push(format!(
                "{} has {} dependencies (max: {})",
                name, count, max
            ));
        }
    }

    RuleResult {
        name: "max-service-dependencies".to_string(),
        passed: violations.is_empty(),
        violations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with(rule: &str, value: RuleValue) -> RulesConfig {
        let mut rules = BTreeMap::new();
        rules.insert(rule.to_string(), value);
        RulesConfig { rules }
    }

    #[test]
    fn test_no_circular_deps_pass() {
        let mut spec = ProjectSpec::new("test".to_string());
        spec.dependencies.push(DependencyEdge {
            from: "a".to_string(),
            to: "b".to_string(),
            kind: DependencyKind::Calls,
            references: Vec::new(),
        });
        spec.dependencies.push(DependencyEdge {
            from: "b".to_string(),
            to: "c".to_string(),
            kind: DependencyKind::Calls,
            references: Vec::new(),
        });

        let config = config_with("no-circular-dependencies", RuleValue::Enabled(true));
        let results = check_rules(&spec, &config);
        assert!(results[0].passed);
    }

    #[test]
    fn test_no_circular_deps_fail() {
        let mut spec = ProjectSpec::new("test".to_string());
        spec.dependencies.push(DependencyEdge {
            from: "a".to_string(),
            to: "b".to_string(),
            kind: DependencyKind::Calls,
            references: Vec::new(),
        });
        spec.dependencies.push(DependencyEdge {
            from: "b".to_string(),
            to: "a".to_string(),
            kind: DependencyKind::Calls,
            references: Vec::new(),
        });

        let config = config_with("no-circular-dependencies", RuleValue::Enabled(true));
        let results = check_rules(&spec, &config);
        assert!(!results[0].passed);
        assert!(results[0].violations[0].contains("Cycle"));
    }

    #[test]
    fn test_controllers_no_db_pass() {
        let mut spec = ProjectSpec::new("test".to_string());
        let mut controller = Capability::new("users".to_string(), "controller.ts".to_string());
        controller.endpoints.push(Endpoint {
            method: HttpMethod::Get,
            path: "/users".to_string(),
            input: None,
            validation: Vec::new(),
            behaviors: Vec::new(),
            security: None,
        });
        let service = Capability::new("users-service".to_string(), "service.ts".to_string());
        spec.capabilities.push(controller);
        spec.capabilities.push(service);
        spec.dependencies.push(DependencyEdge {
            from: "users".to_string(),
            to: "users-service".to_string(),
            kind: DependencyKind::Calls,
            references: Vec::new(),
        });

        let config = config_with("controllers-no-direct-db-access", RuleValue::Enabled(true));
        let results = check_rules(&spec, &config);
        assert!(results[0].passed);
    }

    #[test]
    fn test_controllers_no_db_fail() {
        let mut spec = ProjectSpec::new("test".to_string());
        let mut controller = Capability::new("users".to_string(), "controller.ts".to_string());
        controller.endpoints.push(Endpoint {
            method: HttpMethod::Get,
            path: "/users".to_string(),
            input: None,
            validation: Vec::new(),
            behaviors: Vec::new(),
            security: None,
        });
        let mut entity = Capability::new("user-entity".to_string(), "entity.ts".to_string());
        entity.entities.push(Entity {
            name: "User".to_string(),
            table: "users".to_string(),
            fields: Vec::new(),
            bases: Vec::new(),
        });
        spec.capabilities.push(controller);
        spec.capabilities.push(entity);
        spec.dependencies.push(DependencyEdge {
            from: "users".to_string(),
            to: "user-entity".to_string(),
            kind: DependencyKind::Queries,
            references: Vec::new(),
        });

        let config = config_with("controllers-no-direct-db-access", RuleValue::Enabled(true));
        let results = check_rules(&spec, &config);
        assert!(!results[0].passed);
    }

    #[test]
    fn test_all_endpoints_auth_fail() {
        let mut spec = ProjectSpec::new("test".to_string());
        let mut cap = Capability::new("users".to_string(), "controller.ts".to_string());
        cap.endpoints.push(Endpoint {
            method: HttpMethod::Post,
            path: "/users".to_string(),
            input: None,
            validation: Vec::new(),
            behaviors: Vec::new(),
            security: None, // no auth!
        });
        spec.capabilities.push(cap);

        let config = config_with(
            "all-endpoints-need-authentication",
            RuleValue::Enabled(true),
        );
        let results = check_rules(&spec, &config);
        assert!(!results[0].passed);
        assert!(results[0].violations[0].contains("POST"));
    }

    #[test]
    fn test_max_service_deps_pass() {
        let mut spec = ProjectSpec::new("test".to_string());
        let mut svc = Capability::new("order-service".to_string(), "service.ts".to_string());
        svc.operations.push(Operation {
            name: "create".to_string(),
            source_method: "OrderService#create".to_string(),
            input: None,
            behaviors: Vec::new(),
            transaction: None,
        });
        spec.capabilities.push(svc);
        spec.dependencies.push(DependencyEdge {
            from: "order-service".to_string(),
            to: "repo".to_string(),
            kind: DependencyKind::Calls,
            references: Vec::new(),
        });

        let config = config_with("max-service-dependencies", RuleValue::Threshold(5));
        let results = check_rules(&spec, &config);
        assert!(results[0].passed);
    }

    #[test]
    fn test_max_service_deps_fail() {
        let mut spec = ProjectSpec::new("test".to_string());
        let mut svc = Capability::new("order-service".to_string(), "service.ts".to_string());
        svc.operations.push(Operation {
            name: "create".to_string(),
            source_method: "OrderService#create".to_string(),
            input: None,
            behaviors: Vec::new(),
            transaction: None,
        });
        spec.capabilities.push(svc);

        // Add 3 deps (max is 2)
        for i in 0..3 {
            spec.dependencies.push(DependencyEdge {
                from: "order-service".to_string(),
                to: format!("dep-{}", i),
                kind: DependencyKind::Calls,
                references: Vec::new(),
            });
        }

        let config = config_with("max-service-dependencies", RuleValue::Threshold(2));
        let results = check_rules(&spec, &config);
        assert!(!results[0].passed);
        assert!(results[0].violations[0].contains("3 dependencies"));
    }

    #[test]
    fn test_disabled_rule_skipped() {
        let spec = ProjectSpec::new("test".to_string());
        let config = config_with("no-circular-dependencies", RuleValue::Enabled(false));
        let results = check_rules(&spec, &config);
        assert!(results.is_empty());
    }

    #[test]
    fn test_default_config() {
        let config = RulesConfig::default();
        assert!(config.rules.contains_key("no-circular-dependencies"));
        assert!(config.rules.contains_key("max-service-dependencies"));
        // no-entity-without-validation should be disabled by default
        assert!(!config.rules["no-entity-without-validation"].is_enabled());
    }
}
