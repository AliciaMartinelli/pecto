use crate::model::ProjectSpec;

/// Generate a compact, LLM-optimized context string from a ProjectSpec.
/// Designed to fit in ~4K tokens while capturing the essential behavior.
pub fn to_context(spec: &ProjectSpec) -> String {
    let mut out = String::new();

    // Header
    out.push_str(&format!(
        "Project: {} ({} files, {} capabilities)\n",
        spec.name,
        spec.files_analyzed,
        spec.capabilities.len()
    ));

    // Domains summary
    if !spec.domains.is_empty() {
        let domain_names: Vec<&str> = spec.domains.iter().map(|d| d.name.as_str()).collect();
        out.push_str(&format!("Domains: {}\n", domain_names.join(", ")));
    }

    // Dependencies summary
    if !spec.dependencies.is_empty() {
        out.push_str(&format!(
            "Dependencies: {} edges\n",
            spec.dependencies.len()
        ));
    }

    out.push('\n');

    // Group by domain if available, otherwise flat list
    if !spec.domains.is_empty() {
        for domain in &spec.domains {
            out.push_str(&format!("## {}\n", domain.name));

            for cap_name in &domain.capabilities {
                if let Some(cap) = spec.capabilities.iter().find(|c| &c.name == cap_name) {
                    format_capability(&mut out, cap);
                }
            }

            if !domain.external_dependencies.is_empty() {
                out.push_str(&format!(
                    "  Depends on: {}\n",
                    domain.external_dependencies.join(", ")
                ));
            }
            out.push('\n');
        }

        // Capabilities not in any domain
        let domain_caps: std::collections::HashSet<&str> = spec
            .domains
            .iter()
            .flat_map(|d| d.capabilities.iter().map(|c| c.as_str()))
            .collect();

        let orphans: Vec<_> = spec
            .capabilities
            .iter()
            .filter(|c| !domain_caps.contains(c.name.as_str()))
            .collect();

        if !orphans.is_empty() {
            out.push_str("## Other\n");
            for cap in orphans {
                format_capability(&mut out, cap);
            }
            out.push('\n');
        }
    } else {
        for cap in &spec.capabilities {
            format_capability(&mut out, cap);
        }
    }

    // Key dependencies
    if !spec.dependencies.is_empty() {
        out.push_str("## Dependencies\n");
        for dep in &spec.dependencies {
            out.push_str(&format!("  {} → {} ({:?})\n", dep.from, dep.to, dep.kind));
        }
    }

    out
}

fn format_capability(out: &mut String, cap: &crate::model::Capability) {
    if !cap.endpoints.is_empty() {
        for ep in &cap.endpoints {
            out.push_str(&format!("  {:?} {}", ep.method, ep.path));
            if let Some(sec) = &ep.security
                && sec.authentication.is_some()
            {
                out.push_str(" [auth]");
            }
            out.push('\n');

            // Validation summary
            if !ep.validation.is_empty() {
                let fields: Vec<&str> = ep.validation.iter().map(|v| v.field.as_str()).collect();
                out.push_str(&format!("    validates: {}\n", fields.join(", ")));
            }

            // Behaviors beyond success
            for b in &ep.behaviors {
                if b.name != "success" {
                    out.push_str(&format!("    {} → {}\n", b.name, b.returns.status));
                }
            }
        }
    }

    if !cap.entities.is_empty() {
        for entity in &cap.entities {
            let field_names: Vec<&str> = entity.fields.iter().map(|f| f.name.as_str()).collect();
            out.push_str(&format!(
                "  Entity {} (table: {}) [{}]\n",
                entity.name,
                entity.table,
                field_names.join(", ")
            ));
        }
    }

    if !cap.operations.is_empty() {
        for op in &cap.operations {
            out.push_str(&format!("  {}", op.name));
            if let Some(tx) = &op.transaction {
                out.push_str(&format!(" [tx:{}]", tx));
            }
            let effects: Vec<String> = op
                .behaviors
                .iter()
                .flat_map(|b| &b.side_effects)
                .map(format_side_effect)
                .collect();
            if !effects.is_empty() {
                out.push_str(&format!(" → {}", effects.join(", ")));
            }
            out.push('\n');
        }
    }

    if !cap.scheduled_tasks.is_empty() {
        for task in &cap.scheduled_tasks {
            out.push_str(&format!("  Scheduled: {} ({})\n", task.name, task.schedule));
        }
    }
}

fn format_side_effect(effect: &crate::model::SideEffect) -> String {
    match effect {
        crate::model::SideEffect::DbInsert { table } => format!("insert:{}", table),
        crate::model::SideEffect::DbUpdate { description } => format!("update:{}", description),
        crate::model::SideEffect::Event { name } => format!("event:{}", name),
        crate::model::SideEffect::ServiceCall { target } => format!("call:{}", target),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    #[test]
    fn test_context_export_basic() {
        let mut spec = ProjectSpec::new("my-app");
        spec.files_analyzed = 50;

        let mut cap = Capability::new("user-controller", "UserController.java");
        cap.endpoints.push(Endpoint {
            method: HttpMethod::Get,
            path: "/api/users".to_string(),
            input: None,
            validation: Vec::new(),
            behaviors: vec![Behavior {
                name: "success".to_string(),
                condition: None,
                returns: ResponseSpec {
                    status: 200,
                    body: None,
                },
                side_effects: Vec::new(),
            }],
            security: None,
        });
        spec.capabilities.push(cap);

        let ctx = to_context(&spec);
        assert!(ctx.contains("my-app"));
        assert!(ctx.contains("50 files"));
        // HttpMethod uses Debug format (Get, Post, etc.)
        assert!(
            ctx.contains("/api/users"),
            "Should contain endpoint path. Got: {}",
            ctx
        );
    }

    #[test]
    fn test_context_export_with_domains() {
        let mut spec = ProjectSpec::new("test");
        spec.files_analyzed = 10;

        spec.capabilities
            .push(Capability::new("user-service", "UserService.java"));
        spec.domains.push(Domain {
            name: "user".to_string(),
            capabilities: vec!["user-service".to_string()],
            external_dependencies: vec![],
        });

        let ctx = to_context(&spec);
        assert!(ctx.contains("Domains: user"));
        assert!(ctx.contains("## user"));
    }
}
