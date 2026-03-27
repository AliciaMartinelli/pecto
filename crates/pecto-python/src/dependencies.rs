use crate::context::AnalysisContext;
use crate::extractors::common::*;
use pecto_core::model::*;

/// Resolve dependencies between Python capabilities by analyzing import statements.
pub fn resolve_dependencies(spec: &mut ProjectSpec, ctx: &AnalysisContext) {
    for file in &ctx.files {
        let root = file.tree.root_node();
        let source = file.source.as_bytes();

        // Find which capability this file belongs to
        let Some(from_cap) = spec
            .capabilities
            .iter()
            .find(|c| c.source == file.path)
            .map(|c| c.name.clone())
        else {
            continue;
        };

        // Extract import statements
        for i in 0..root.named_child_count() {
            let node = root.named_child(i).unwrap();

            match node.kind() {
                // from app.models import Item, ItemCreate
                "import_from_statement" => {
                    let module = node
                        .child_by_field_name("module_name")
                        .map(|m| node_text(&m, source))
                        .unwrap_or_default();

                    if module.is_empty() {
                        continue;
                    }

                    // Try to match the module path to a capability
                    if let Some(to_cap) =
                        resolve_module_to_capability(&module, &from_cap, &spec.capabilities)
                    {
                        // Extract imported names for reference
                        let mut imported_names = Vec::new();
                        for j in 0..node.named_child_count() {
                            let child = node.named_child(j).unwrap();
                            if child.kind() == "dotted_name" || child.kind() == "aliased_import" {
                                imported_names.push(node_text(&child, source));
                            }
                        }

                        let reference = format!(
                            "from {} import {}",
                            module,
                            if imported_names.is_empty() {
                                "*".to_string()
                            } else {
                                imported_names.join(", ")
                            }
                        );

                        add_dependency_edge(
                            &mut spec.dependencies,
                            &from_cap,
                            &to_cap,
                            infer_dependency_kind(&to_cap, &module),
                            reference,
                        );
                    }
                }
                // import app.models
                "import_statement" => {
                    let text = node_text(&node, source);
                    let module = text.trim_start_matches("import ").trim();

                    if let Some(to_cap) =
                        resolve_module_to_capability(module, &from_cap, &spec.capabilities)
                    {
                        add_dependency_edge(
                            &mut spec.dependencies,
                            &from_cap,
                            &to_cap,
                            infer_dependency_kind(&to_cap, module),
                            text,
                        );
                    }
                }
                _ => {}
            }
        }
    }
}

/// Resolve a Python module path to a capability name.
/// e.g., "app.api.deps" or "app.models" → matching capability
fn resolve_module_to_capability(
    module: &str,
    from_cap: &str,
    capabilities: &[Capability],
) -> Option<String> {
    // Convert module path to file path patterns:
    // "app.models" → "app/models.py" or "app/models/__init__.py"
    let module_as_path = module.replace('.', "/");

    for cap in capabilities {
        if cap.name == from_cap {
            continue; // Skip self-references
        }

        // Check if capability source matches the module path
        // e.g., source="backend/app/api/routes/items.py" matches "app.api.routes.items"
        let source_no_ext = cap.source.trim_end_matches(".py");

        if source_no_ext.ends_with(&module_as_path)
            || source_no_ext.contains(&module_as_path)
            || module_path_matches_source(module, &cap.source)
        {
            return Some(cap.name.clone());
        }
    }

    // Try matching by capability name against module segments
    // e.g., module "app.models" with capability "private-model" (source contains "models")
    let module_segments: Vec<&str> = module.split('.').collect();
    let last_segment = module_segments.last().unwrap_or(&"");

    for cap in capabilities {
        if cap.name == from_cap {
            continue;
        }

        // Match last module segment against capability name or source
        let cap_source_segments: Vec<&str> = cap.source.split('/').collect();
        let cap_file = cap_source_segments
            .last()
            .unwrap_or(&"")
            .trim_end_matches(".py");

        if cap_file == *last_segment
            || cap.name == *last_segment
            || cap.name.contains(last_segment)
        {
            return Some(cap.name.clone());
        }
    }

    None
}

/// Check if a module path matches a source file path.
fn module_path_matches_source(module: &str, source: &str) -> bool {
    let source_parts: Vec<&str> = source
        .trim_end_matches(".py")
        .split('/')
        .collect();
    let module_parts: Vec<&str> = module.split('.').collect();

    if module_parts.len() > source_parts.len() {
        return false;
    }

    // Check if module parts match the tail of source parts
    let offset = source_parts.len() - module_parts.len();
    module_parts
        .iter()
        .zip(source_parts[offset..].iter())
        .all(|(m, s)| m == s)
}

fn infer_dependency_kind(target: &str, module: &str) -> DependencyKind {
    if target.contains("model") || module.contains("model") {
        DependencyKind::Queries
    } else if target.contains("event") || module.contains("event") || module.contains("task") {
        DependencyKind::Listens
    } else {
        DependencyKind::Calls
    }
}

fn add_dependency_edge(
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
