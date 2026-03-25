use crate::model::{Domain, ProjectSpec};
use std::collections::{BTreeMap, BTreeSet};

/// Cluster capabilities into domains based on naming conventions and dependencies.
pub fn cluster_domains(spec: &mut ProjectSpec) {
    // Step 1: Group by name prefix (user-controller, user-entity, user-service → "user")
    let mut prefix_groups: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for cap in &spec.capabilities {
        let prefix = extract_domain_prefix(&cap.name);
        prefix_groups
            .entry(prefix)
            .or_default()
            .insert(cap.name.clone());
    }

    // Step 2: Merge small groups that are connected by dependencies
    let merged = merge_connected_groups(&prefix_groups, &spec.dependencies);

    // Step 3: Find external dependencies for each domain
    let mut domains = Vec::new();
    for (domain_name, caps) in &merged {
        let mut external_deps = BTreeSet::new();
        for dep in &spec.dependencies {
            if caps.contains(&dep.from) && !caps.contains(&dep.to) {
                // Find which domain the target belongs to
                for (other_domain, other_caps) in &merged {
                    if other_caps.contains(&dep.to) && other_domain != domain_name {
                        external_deps.insert(other_domain.clone());
                    }
                }
            }
        }

        domains.push(Domain {
            name: domain_name.clone(),
            capabilities: caps.iter().cloned().collect(),
            external_dependencies: external_deps.into_iter().collect(),
        });
    }

    // Sort domains by name for stable output
    domains.sort_by(|a, b| a.name.cmp(&b.name));
    spec.domains = domains;
}

/// Extract domain prefix from a capability name.
/// "user-controller" → "user", "order-service" → "order", "app-db-context" → "app"
fn extract_domain_prefix(name: &str) -> String {
    let suffixes = [
        "-controller",
        "-service",
        "-repository",
        "-entity",
        "-rest",
        "-db-context",
    ];

    for suffix in &suffixes {
        if let Some(prefix) = name.strip_suffix(suffix) {
            return prefix.to_string();
        }
    }

    // No known suffix — use the full name as its own domain
    name.to_string()
}

/// Merge groups that are connected by dependencies.
fn merge_connected_groups(
    groups: &BTreeMap<String, BTreeSet<String>>,
    dependencies: &[crate::model::DependencyEdge],
) -> BTreeMap<String, BTreeSet<String>> {
    let mut result = groups.clone();

    // For single-capability groups, try to merge into a connected group
    let small_groups: Vec<String> = result
        .iter()
        .filter(|(_, caps)| caps.len() == 1)
        .map(|(name, _)| name.clone())
        .collect();

    for small_name in &small_groups {
        let small_caps = result.get(small_name).cloned().unwrap_or_default();
        let cap_name = small_caps.iter().next().cloned().unwrap_or_default();

        // Find if this capability has a dependency to/from a larger group
        let mut best_target: Option<String> = None;
        for dep in dependencies {
            let other = if dep.from == cap_name {
                &dep.to
            } else if dep.to == cap_name {
                &dep.from
            } else {
                continue;
            };

            // Find which group the other capability belongs to
            for (group_name, group_caps) in &result {
                if group_caps.contains(other) && group_name != small_name && group_caps.len() > 1 {
                    best_target = Some(group_name.clone());
                    break;
                }
            }

            if best_target.is_some() {
                break;
            }
        }

        // Merge into the target group
        if let Some(target) = best_target {
            let caps_to_move = result.remove(small_name).unwrap_or_default();
            if let Some(target_caps) = result.get_mut(&target) {
                target_caps.extend(caps_to_move);
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    #[test]
    fn test_domain_clustering_by_prefix() {
        let mut spec = ProjectSpec::new("test");
        spec.capabilities
            .push(Capability::new("user-controller", "UserController.java"));
        spec.capabilities
            .push(Capability::new("user-entity", "User.java"));
        spec.capabilities
            .push(Capability::new("user-service", "UserService.java"));
        spec.capabilities
            .push(Capability::new("order-controller", "OrderController.java"));
        spec.capabilities
            .push(Capability::new("order-entity", "Order.java"));

        cluster_domains(&mut spec);

        assert_eq!(spec.domains.len(), 2);

        let user_domain = spec.domains.iter().find(|d| d.name == "user").unwrap();
        assert_eq!(user_domain.capabilities.len(), 3);

        let order_domain = spec.domains.iter().find(|d| d.name == "order").unwrap();
        assert_eq!(order_domain.capabilities.len(), 2);
    }

    #[test]
    fn test_domain_external_dependencies() {
        let mut spec = ProjectSpec::new("test");
        spec.capabilities
            .push(Capability::new("order-service", "OrderService.java"));
        spec.capabilities
            .push(Capability::new("user-service", "UserService.java"));
        spec.dependencies.push(DependencyEdge {
            from: "order-service".to_string(),
            to: "user-service".to_string(),
            kind: DependencyKind::Calls,
            references: vec![],
        });

        cluster_domains(&mut spec);

        let order_domain = spec.domains.iter().find(|d| d.name == "order").unwrap();
        assert!(
            order_domain
                .external_dependencies
                .contains(&"user".to_string())
        );
    }
}
