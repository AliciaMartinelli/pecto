use crate::model::*;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

/// Generate a GitHub-flavored Markdown diff between two ProjectSpecs.
pub fn generate_pr_diff(base: &ProjectSpec, head: &ProjectSpec) -> String {
    let mut md = String::new();

    let ep_diff = diff_endpoints(base, head);
    let ent_diff = diff_entities(base, head);
    let dep_diff = diff_dependencies(base, head);
    let flow_diff = diff_flows(base, head);
    let cap_diff = diff_capabilities(base, head);

    let has_changes = !ep_diff.added.is_empty()
        || !ep_diff.removed.is_empty()
        || !ep_diff.modified.is_empty()
        || !ent_diff.added.is_empty()
        || !ent_diff.removed.is_empty()
        || !ent_diff.modified.is_empty()
        || !dep_diff.added.is_empty()
        || !dep_diff.removed.is_empty()
        || !flow_diff.added.is_empty()
        || !flow_diff.removed.is_empty()
        || !flow_diff.modified.is_empty()
        || !cap_diff.added.is_empty()
        || !cap_diff.removed.is_empty();

    // Header
    let _ = writeln!(md, "## 🔍 pecto — Behavior Diff\n");

    if !has_changes {
        let _ = writeln!(md, "No behavior changes detected.\n");
        let _ = writeln!(md, "<!-- pecto -->");
        return md;
    }

    // Summary line
    let mut summary_parts = Vec::new();
    let ep_total = ep_diff.added.len() + ep_diff.modified.len() + ep_diff.removed.len();
    if ep_total > 0 {
        summary_parts.push(format_summary_part(
            "endpoint",
            ep_diff.added.len(),
            ep_diff.modified.len(),
            ep_diff.removed.len(),
        ));
    }
    let ent_total = ent_diff.added.len() + ent_diff.modified.len() + ent_diff.removed.len();
    if ent_total > 0 {
        summary_parts.push(format_summary_part(
            "entity",
            ent_diff.added.len(),
            ent_diff.modified.len(),
            ent_diff.removed.len(),
        ));
    }
    let dep_total = dep_diff.added.len() + dep_diff.removed.len();
    if dep_total > 0 {
        let mut s = Vec::new();
        if !dep_diff.added.is_empty() {
            s.push(format!("**{}** added", dep_diff.added.len()));
        }
        if !dep_diff.removed.is_empty() {
            s.push(format!("**{}** removed", dep_diff.removed.len()));
        }
        summary_parts.push(format!("{} (dependencies)", s.join(", ")));
    }
    if !cap_diff.added.is_empty() || !cap_diff.removed.is_empty() {
        let mut s = Vec::new();
        if !cap_diff.added.is_empty() {
            s.push(format!("**{}** added", cap_diff.added.len()));
        }
        if !cap_diff.removed.is_empty() {
            s.push(format!("**{}** removed", cap_diff.removed.len()));
        }
        summary_parts.push(format!("{} (capabilities)", s.join(", ")));
    }

    if !summary_parts.is_empty() {
        let _ = writeln!(md, "{}\n", summary_parts.join(" · "));
    }

    // Capabilities section (new/removed)
    if !cap_diff.added.is_empty() || !cap_diff.removed.is_empty() {
        let _ = writeln!(md, "### Capabilities\n");
        let _ = writeln!(md, "| Status | Name | Type |");
        let _ = writeln!(md, "|--------|------|------|");
        for (name, cap_type) in &cap_diff.added {
            let _ = writeln!(md, "| 🆕 | `{}` | {} |", name, cap_type);
        }
        for (name, cap_type) in &cap_diff.removed {
            let _ = writeln!(md, "| 🗑️ | ~~`{}`~~ | {} |", name, cap_type);
        }
        let _ = writeln!(md);
    }

    // Endpoints section
    if !ep_diff.added.is_empty() || !ep_diff.removed.is_empty() || !ep_diff.modified.is_empty() {
        let _ = writeln!(md, "### Endpoints\n");
        let _ = writeln!(md, "| Status | Method | Path | Capability |");
        let _ = writeln!(md, "|--------|--------|------|------------|");
        for ep in &ep_diff.added {
            let _ = writeln!(
                md,
                "| 🆕 | **{}** | `{}` | {} |",
                ep.method, ep.path, ep.capability
            );
        }
        for ep in &ep_diff.modified {
            let _ = writeln!(
                md,
                "| ✏️ | **{}** | `{}` | {} |",
                ep.method, ep.path, ep.capability
            );
        }
        for ep in &ep_diff.removed {
            let _ = writeln!(
                md,
                "| 🗑️ | ~~**{}**~~ | ~~`{}`~~ | {} |",
                ep.method, ep.path, ep.capability
            );
        }
        let _ = writeln!(md);
    }

    // Entities section
    if !ent_diff.added.is_empty()
        || !ent_diff.removed.is_empty()
        || !ent_diff.modified.is_empty()
    {
        let _ = writeln!(md, "### Entities\n");
        let _ = writeln!(md, "| Status | Entity | Details |");
        let _ = writeln!(md, "|--------|--------|---------|");
        for ent in &ent_diff.added {
            let _ = writeln!(md, "| 🆕 | `{}` | {} fields |", ent.name, ent.detail);
        }
        for ent in &ent_diff.modified {
            let _ = writeln!(md, "| ✏️ | `{}` | {} |", ent.name, ent.detail);
        }
        for ent in &ent_diff.removed {
            let _ = writeln!(md, "| 🗑️ | ~~`{}`~~ | removed |", ent.name);
        }
        let _ = writeln!(md);
    }

    // Dependencies section
    if !dep_diff.added.is_empty() || !dep_diff.removed.is_empty() {
        let _ = writeln!(md, "### Dependencies\n");
        let _ = writeln!(md, "| Status | From | | To | Kind |");
        let _ = writeln!(md, "|--------|------|-|-----|------|");
        for dep in &dep_diff.added {
            let _ = writeln!(md, "| 🆕 | `{}` | → | `{}` | {} |", dep.from, dep.to, dep.kind);
        }
        for dep in &dep_diff.removed {
            let _ = writeln!(
                md,
                "| 🗑️ | ~~`{}`~~ | → | ~~`{}`~~ | {} |",
                dep.from, dep.to, dep.kind
            );
        }
        let _ = writeln!(md);
    }

    // Flow changes section
    if !flow_diff.added.is_empty()
        || !flow_diff.removed.is_empty()
        || !flow_diff.modified.is_empty()
    {
        let _ = writeln!(md, "### Flow Changes\n");
        for f in &flow_diff.added {
            let _ = writeln!(
                md,
                "- 🆕 **{}**: new flow ({} steps)",
                f.trigger, f.detail
            );
        }
        for f in &flow_diff.modified {
            let _ = writeln!(md, "- ✏️ **{}**: {}", f.trigger, f.detail);
        }
        for f in &flow_diff.removed {
            let _ = writeln!(md, "- 🗑️ ~~**{}**~~: flow removed", f.trigger);
        }
        let _ = writeln!(md);
    }

    // Marker for GitHub Action update-mode
    let _ = writeln!(md, "<!-- pecto -->");

    md
}

// ─── Diff types ───

struct EndpointDiff {
    added: Vec<EndpointEntry>,
    removed: Vec<EndpointEntry>,
    modified: Vec<EndpointEntry>,
}

#[derive(Clone)]
struct EndpointEntry {
    method: String,
    path: String,
    capability: String,
}

struct EntityDiff {
    added: Vec<EntityEntry>,
    removed: Vec<EntityEntry>,
    modified: Vec<EntityEntry>,
}

struct EntityEntry {
    name: String,
    detail: String,
}

struct DepDiff {
    added: Vec<DepEntry>,
    removed: Vec<DepEntry>,
}

struct DepEntry {
    from: String,
    to: String,
    kind: String,
}

struct FlowDiff {
    added: Vec<FlowEntry>,
    removed: Vec<FlowEntry>,
    modified: Vec<FlowEntry>,
}

struct FlowEntry {
    trigger: String,
    detail: String,
}

struct CapDiff {
    added: Vec<(String, String)>,   // (name, type)
    removed: Vec<(String, String)>,
}

// ─── Diff logic ───

fn diff_endpoints(base: &ProjectSpec, head: &ProjectSpec) -> EndpointDiff {
    let base_eps = collect_endpoints(base);
    let head_eps = collect_endpoints(head);

    let base_keys: BTreeSet<_> = base_eps.keys().collect();
    let head_keys: BTreeSet<_> = head_eps.keys().collect();

    let added = head_keys
        .difference(&base_keys)
        .map(|k| head_eps[*k].clone())
        .collect();
    let removed = base_keys
        .difference(&head_keys)
        .map(|k| base_eps[*k].clone())
        .collect();

    // Modified: same key but different behaviors/security/validation
    let modified = base_keys
        .intersection(&head_keys)
        .filter(|k| {
            let b = &base_eps[**k];
            let h = &head_eps[**k];
            b.capability != h.capability // capability moved
        })
        .map(|k| head_eps[*k].clone())
        .collect();

    EndpointDiff {
        added,
        removed,
        modified,
    }
}

fn collect_endpoints(spec: &ProjectSpec) -> BTreeMap<String, EndpointEntry> {
    let mut map = BTreeMap::new();
    for cap in &spec.capabilities {
        for ep in &cap.endpoints {
            let method = format!("{:?}", ep.method).to_uppercase();
            let key = format!("{} {}", method, ep.path);
            map.insert(
                key,
                EndpointEntry {
                    method,
                    path: ep.path.clone(),
                    capability: cap.name.clone(),
                },
            );
        }
    }
    map
}

fn diff_entities(base: &ProjectSpec, head: &ProjectSpec) -> EntityDiff {
    let base_ents = collect_entities(base);
    let head_ents = collect_entities(head);

    let base_keys: BTreeSet<_> = base_ents.keys().cloned().collect();
    let head_keys: BTreeSet<_> = head_ents.keys().cloned().collect();

    let added = head_keys
        .difference(&base_keys)
        .map(|name| {
            let fields = &head_ents[name];
            EntityEntry {
                name: name.clone(),
                detail: fields.len().to_string(),
            }
        })
        .collect();

    let removed = base_keys
        .difference(&head_keys)
        .map(|name| EntityEntry {
            name: name.clone(),
            detail: String::new(),
        })
        .collect();

    let modified = base_keys
        .intersection(&head_keys)
        .filter_map(|name| {
            let base_fields: BTreeSet<_> = base_ents[name].iter().cloned().collect();
            let head_fields: BTreeSet<_> = head_ents[name].iter().cloned().collect();
            if base_fields == head_fields {
                return None;
            }
            let added: Vec<_> = head_fields.difference(&base_fields).collect();
            let removed: Vec<_> = base_fields.difference(&head_fields).collect();
            let mut parts = Vec::new();
            for f in &added {
                parts.push(format!("+{}", f));
            }
            for f in &removed {
                parts.push(format!("-{}", f));
            }
            Some(EntityEntry {
                name: name.clone(),
                detail: parts.join(", "),
            })
        })
        .collect();

    EntityDiff {
        added,
        removed,
        modified,
    }
}

fn collect_entities(spec: &ProjectSpec) -> BTreeMap<String, Vec<String>> {
    let mut map = BTreeMap::new();
    for cap in &spec.capabilities {
        for ent in &cap.entities {
            let fields: Vec<String> = ent.fields.iter().map(|f| f.name.clone()).collect();
            map.insert(ent.name.clone(), fields);
        }
    }
    map
}

fn diff_dependencies(base: &ProjectSpec, head: &ProjectSpec) -> DepDiff {
    let base_deps: BTreeSet<_> = base
        .dependencies
        .iter()
        .map(|d| (d.from.clone(), d.to.clone(), format!("{:?}", d.kind)))
        .collect();
    let head_deps: BTreeSet<_> = head
        .dependencies
        .iter()
        .map(|d| (d.from.clone(), d.to.clone(), format!("{:?}", d.kind)))
        .collect();

    let added = head_deps
        .difference(&base_deps)
        .map(|(f, t, k)| DepEntry {
            from: f.clone(),
            to: t.clone(),
            kind: k.clone(),
        })
        .collect();
    let removed = base_deps
        .difference(&head_deps)
        .map(|(f, t, k)| DepEntry {
            from: f.clone(),
            to: t.clone(),
            kind: k.clone(),
        })
        .collect();

    DepDiff { added, removed }
}

fn diff_flows(base: &ProjectSpec, head: &ProjectSpec) -> FlowDiff {
    let base_flows: BTreeMap<_, _> = base.flows.iter().map(|f| (f.trigger.clone(), f)).collect();
    let head_flows: BTreeMap<_, _> = head.flows.iter().map(|f| (f.trigger.clone(), f)).collect();

    let base_keys: BTreeSet<_> = base_flows.keys().cloned().collect();
    let head_keys: BTreeSet<_> = head_flows.keys().cloned().collect();

    let added = head_keys
        .difference(&base_keys)
        .map(|t| {
            let flow = head_flows[t];
            FlowEntry {
                trigger: t.clone(),
                detail: count_steps(&flow.steps).to_string(),
            }
        })
        .collect();

    let removed = head_keys
        .difference(&base_keys)
        .map(|t| FlowEntry {
            trigger: t.clone(),
            detail: String::new(),
        })
        .collect();

    let modified = base_keys
        .intersection(&head_keys)
        .filter_map(|t| {
            let base_count = count_steps(&base_flows[t].steps);
            let head_count = count_steps(&head_flows[t].steps);
            if base_count == head_count {
                return None;
            }
            let diff = head_count as i32 - base_count as i32;
            let detail = if diff > 0 {
                format!("+{} steps ({} → {})", diff, base_count, head_count)
            } else {
                format!("{} steps ({} → {})", diff, base_count, head_count)
            };
            Some(FlowEntry {
                trigger: t.clone(),
                detail,
            })
        })
        .collect();

    FlowDiff {
        added,
        removed,
        modified,
    }
}

fn count_steps(steps: &[FlowStep]) -> usize {
    steps
        .iter()
        .map(|s| 1 + count_steps(&s.children))
        .sum()
}

fn diff_capabilities(base: &ProjectSpec, head: &ProjectSpec) -> CapDiff {
    let base_names: BTreeSet<_> = base.capabilities.iter().map(|c| c.name.clone()).collect();
    let head_names: BTreeSet<_> = head.capabilities.iter().map(|c| c.name.clone()).collect();

    let added = head_names
        .difference(&base_names)
        .map(|name| {
            let cap = head.capabilities.iter().find(|c| &c.name == name).unwrap();
            (name.clone(), cap_type_label(cap))
        })
        .collect();

    let removed = base_names
        .difference(&head_names)
        .map(|name| {
            let cap = base.capabilities.iter().find(|c| &c.name == name).unwrap();
            (name.clone(), cap_type_label(cap))
        })
        .collect();

    CapDiff { added, removed }
}

fn cap_type_label(cap: &Capability) -> String {
    if !cap.endpoints.is_empty() {
        "Controller".to_string()
    } else if !cap.entities.is_empty() {
        "Entity".to_string()
    } else if !cap.operations.is_empty() {
        "Service".to_string()
    } else if !cap.scheduled_tasks.is_empty() {
        "Scheduled".to_string()
    } else {
        "Other".to_string()
    }
}

fn format_summary_part(noun: &str, added: usize, modified: usize, removed: usize) -> String {
    let mut parts = Vec::new();
    if added > 0 {
        parts.push(format!("**{}** added", added));
    }
    if modified > 0 {
        parts.push(format!("**{}** modified", modified));
    }
    if removed > 0 {
        parts.push(format!("**{}** removed", removed));
    }
    let plural = if added + modified + removed != 1 {
        "s"
    } else {
        ""
    };
    format!("{} ({}{})", parts.join(", "), noun, plural)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_changes() {
        let spec = ProjectSpec::new("test".to_string());
        let md = generate_pr_diff(&spec, &spec);
        assert!(md.contains("No behavior changes detected"));
        assert!(md.contains("<!-- pecto -->"));
    }

    #[test]
    fn test_new_endpoint() {
        let base = ProjectSpec::new("test".to_string());
        let mut head = ProjectSpec::new("test".to_string());
        let mut cap = Capability::new("users".to_string(), "users.py".to_string());
        cap.endpoints.push(Endpoint {
            method: HttpMethod::Post,
            path: "/users".to_string(),
            input: None,
            validation: Vec::new(),
            behaviors: Vec::new(),
            security: None,
        });
        head.capabilities.push(cap);

        let md = generate_pr_diff(&base, &head);
        assert!(md.contains("POST"));
        assert!(md.contains("/users"));
        assert!(md.contains("🆕"));
        assert!(md.contains("<!-- pecto -->"));
    }

    #[test]
    fn test_new_entity() {
        let base = ProjectSpec::new("test".to_string());
        let mut head = ProjectSpec::new("test".to_string());
        let mut cap = Capability::new("models".to_string(), "models.py".to_string());
        cap.entities.push(Entity {
            name: "User".to_string(),
            table: "users".to_string(),
            fields: vec![
                EntityField {
                    name: "id".to_string(),
                    field_type: "int".to_string(),
                    constraints: Vec::new(),
                },
                EntityField {
                    name: "email".to_string(),
                    field_type: "str".to_string(),
                    constraints: Vec::new(),
                },
            ],
            bases: Vec::new(),
        });
        head.capabilities.push(cap);

        let md = generate_pr_diff(&base, &head);
        assert!(md.contains("User"));
        assert!(md.contains("2 fields"));
    }

    #[test]
    fn test_removed_dependency() {
        let mut base = ProjectSpec::new("test".to_string());
        base.dependencies.push(DependencyEdge {
            from: "controller".to_string(),
            to: "service".to_string(),
            kind: DependencyKind::Calls,
            references: Vec::new(),
        });
        let head = ProjectSpec::new("test".to_string());

        let md = generate_pr_diff(&base, &head);
        assert!(md.contains("controller"));
        assert!(md.contains("🗑️"));
    }
}
