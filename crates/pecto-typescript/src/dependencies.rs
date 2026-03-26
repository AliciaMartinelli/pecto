use crate::context::AnalysisContext;
use crate::extractors::common::*;
use pecto_core::model::*;

/// Resolve dependencies between TypeScript capabilities via constructor injection and imports.
pub fn resolve_dependencies(spec: &mut ProjectSpec, ctx: &AnalysisContext) {
    let capability_names: Vec<String> = spec.capabilities.iter().map(|c| c.name.clone()).collect();

    for file in &ctx.files {
        let source = &file.source;

        // Find which capability this file belongs to
        let from_cap = spec
            .capabilities
            .iter()
            .find(|c| c.source == file.path)
            .map(|c| c.name.clone());

        let Some(from) = from_cap else {
            continue;
        };

        // Strategy 1: Constructor injection
        // constructor(private usersService: UsersService)
        for line in source.lines() {
            let trimmed = line.trim();
            if !trimmed.contains("private ")
                && !trimmed.contains("readonly ")
                && !trimmed.contains("constructor(")
            {
                continue;
            }

            for type_name in extract_injected_types(trimmed) {
                if let Some(target) = resolve_type_to_capability(&type_name, &capability_names)
                    && target != from
                {
                    add_dep(
                        &mut spec.dependencies,
                        &from,
                        &target,
                        infer_kind(&target),
                        format!("{} injects {}", from, type_name),
                    );
                }
            }
        }

        // Strategy 2: Import-based dependencies
        for line in source.lines() {
            let trimmed = line.trim();
            if !trimmed.starts_with("import ") {
                continue;
            }

            for name in extract_import_names(trimmed) {
                if let Some(target) = resolve_type_to_capability(&name, &capability_names)
                    && target != from
                {
                    add_dep(
                        &mut spec.dependencies,
                        &from,
                        &target,
                        infer_kind(&target),
                        format!("{} imports {}", from, name),
                    );
                }
            }
        }
    }
}

fn extract_injected_types(line: &str) -> Vec<String> {
    let mut types = Vec::new();

    for part in line.split(',') {
        let trimmed = part
            .trim()
            .trim_start_matches("constructor(")
            .trim_start_matches('(')
            .trim_end_matches(')')
            .trim_end_matches('{')
            .trim();

        if trimmed.contains(':') {
            let type_part = trimmed
                .split(':')
                .next_back()
                .unwrap_or("")
                .trim()
                .trim_end_matches(',')
                .trim_end_matches(')')
                .trim();

            if !type_part.is_empty()
                && type_part.chars().next().is_some_and(|c| c.is_uppercase())
                && !matches!(
                    type_part,
                    "Promise" | "Observable" | "String" | "Number" | "Boolean" | "Record"
                )
                && !type_part.starts_with("Promise<")
                && !type_part.starts_with("Observable<")
            {
                types.push(type_part.to_string());
            }
        }
    }

    types
}

fn extract_import_names(line: &str) -> Vec<String> {
    let mut names = Vec::new();

    if let Some(start) = line.find('{')
        && let Some(end) = line.find('}')
    {
        let imports = &line[start + 1..end];
        for name in imports.split(',') {
            let trimmed = name.trim().split(" as ").next().unwrap_or("").trim();
            if !trimmed.is_empty() && trimmed.chars().next().is_some_and(|c| c.is_uppercase()) {
                names.push(trimmed.to_string());
            }
        }
    }

    names
}

fn resolve_type_to_capability(type_name: &str, capability_names: &[String]) -> Option<String> {
    let kebab = to_kebab_case(type_name);

    if capability_names.contains(&kebab) {
        return Some(kebab);
    }

    let base = type_name
        .replace("Service", "")
        .replace("Repository", "")
        .replace("Controller", "");
    let kebab_base = to_kebab_case(&base);

    let candidates = [
        format!("{}-service", kebab_base),
        format!("{}-entity", kebab_base),
        kebab_base.clone(),
    ];

    for candidate in &candidates {
        if capability_names.contains(candidate) {
            return Some(candidate.clone());
        }
    }

    for cap in capability_names {
        if cap.starts_with(&kebab_base) && !kebab_base.is_empty() && kebab_base.len() > 2 {
            return Some(cap.clone());
        }
    }

    None
}

fn infer_kind(target: &str) -> DependencyKind {
    if target.contains("repository") || target.contains("entity") {
        DependencyKind::Queries
    } else {
        DependencyKind::Calls
    }
}

fn add_dep(
    deps: &mut Vec<DependencyEdge>,
    from: &str,
    to: &str,
    kind: DependencyKind,
    reference: String,
) {
    if let Some(existing) = deps.iter_mut().find(|d| d.from == from && d.to == to) {
        if !existing.references.contains(&reference) {
            existing.references.push(reference);
        }
    } else {
        deps.push(DependencyEdge {
            from: from.to_string(),
            to: to.to_string(),
            kind,
            references: vec![reference],
        });
    }
}
