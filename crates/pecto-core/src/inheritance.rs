use crate::model::{EntityField, ProjectSpec};
use std::collections::HashMap;

/// Merge inherited fields into entities that have `bases` populated.
///
/// For each entity with parent classes in `bases`, if those parents are also
/// entities in the same spec, prepend the parent's fields (skipping duplicates).
/// Handles inheritance chains (A -> B -> C) across all capabilities.
///
/// Works for all languages:
/// - Python: `class UserCreate(UserBase)` where UserBase has email, is_active
/// - Java: `class Pet extends BaseEntity` where BaseEntity has id
/// - C#: `class Product : BaseEntity` where BaseEntity has Id
/// - TypeScript: `class User extends BaseEntity` where BaseEntity has id
pub fn merge_inherited_fields(spec: &mut ProjectSpec) {
    // Build maps: entity name → fields, entity name → bases
    let mut field_map: HashMap<String, Vec<EntityField>> = HashMap::new();
    let mut bases_map: HashMap<String, Vec<String>> = HashMap::new();

    for capability in &spec.capabilities {
        for entity in &capability.entities {
            field_map.insert(entity.name.clone(), entity.fields.clone());
            if !entity.bases.is_empty() {
                bases_map.insert(entity.name.clone(), entity.bases.clone());
            }
        }
    }

    // Resolve full inherited fields for each entity (with chain support)
    let mut resolved: HashMap<String, Vec<EntityField>> = HashMap::new();

    for capability in &spec.capabilities {
        for entity in &capability.entities {
            if entity.bases.is_empty() {
                continue;
            }

            let inherited = collect_inherited_fields(
                &entity.name,
                &entity.fields,
                &field_map,
                &bases_map,
            );

            if !inherited.is_empty() {
                resolved.insert(entity.name.clone(), inherited);
            }
        }
    }

    // Apply resolved fields
    for capability in &mut spec.capabilities {
        for entity in &mut capability.entities {
            if let Some(mut inherited) = resolved.remove(&entity.name) {
                inherited.append(&mut entity.fields);
                entity.fields = inherited;
            }
        }
    }
}

/// Collect fields from all ancestors, walking up the chain top-down.
/// Returns fields to prepend (parent fields not already in the child).
/// Fields from the most distant ancestor come first (e.g., Base.id before Named.name).
fn collect_inherited_fields(
    entity_name: &str,
    own_fields: &[EntityField],
    field_map: &HashMap<String, Vec<EntityField>>,
    bases_map: &HashMap<String, Vec<String>>,
) -> Vec<EntityField> {
    // First, build the ordered ancestor chain from most distant to most immediate
    let mut ancestor_chain: Vec<String> = Vec::new();
    let mut visited = vec![entity_name.to_string()];
    collect_ancestors(entity_name, bases_map, &mut ancestor_chain, &mut visited);

    // Now collect fields top-down (most distant ancestor first)
    let mut inherited = Vec::new();
    for ancestor in &ancestor_chain {
        if let Some(ancestor_fields) = field_map.get(ancestor) {
            for field in ancestor_fields {
                let already_exists = inherited.iter().any(|f: &EntityField| f.name == field.name)
                    || own_fields.iter().any(|f| f.name == field.name);
                if !already_exists {
                    inherited.push(field.clone());
                }
            }
        }
    }

    inherited
}

/// Recursively collect ancestors in top-down order (most distant first).
fn collect_ancestors(
    entity_name: &str,
    bases_map: &HashMap<String, Vec<String>>,
    chain: &mut Vec<String>,
    visited: &mut Vec<String>,
) {
    let Some(bases) = bases_map.get(entity_name) else {
        return;
    };

    for base in bases {
        if visited.contains(base) {
            continue;
        }
        visited.push(base.clone());

        // Recurse to get grandparent first
        collect_ancestors(base, bases_map, chain, visited);

        // Then add this parent
        chain.push(base.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    fn make_entity(name: &str, fields: Vec<(&str, &str)>, bases: Vec<&str>) -> Entity {
        Entity {
            name: name.to_string(),
            table: name.to_lowercase(),
            fields: fields
                .into_iter()
                .map(|(n, t)| EntityField {
                    name: n.to_string(),
                    field_type: t.to_string(),
                    constraints: Vec::new(),
                })
                .collect(),
            bases: bases.into_iter().map(|b| b.to_string()).collect(),
        }
    }

    #[test]
    fn test_simple_inheritance() {
        let mut spec = ProjectSpec::new("test".to_string());

        let mut cap = Capability::new("models".to_string(), "models.py".to_string());
        cap.entities = vec![
            make_entity("UserBase", vec![("email", "str"), ("is_active", "bool")], vec![]),
            make_entity("UserCreate", vec![("password", "str")], vec!["UserBase"]),
        ];
        spec.capabilities.push(cap);

        merge_inherited_fields(&mut spec);

        let user_create = &spec.capabilities[0].entities[1];
        assert_eq!(user_create.name, "UserCreate");
        assert_eq!(user_create.fields.len(), 3); // email + is_active + password
        assert_eq!(user_create.fields[0].name, "email");
        assert_eq!(user_create.fields[1].name, "is_active");
        assert_eq!(user_create.fields[2].name, "password");
    }

    #[test]
    fn test_chain_inheritance() {
        // A -> B -> C: C should get fields from both A and B
        let mut spec = ProjectSpec::new("test".to_string());

        let mut cap = Capability::new("models".to_string(), "models.py".to_string());
        cap.entities = vec![
            make_entity("Base", vec![("id", "int")], vec![]),
            make_entity("Named", vec![("name", "str")], vec!["Base"]),
            make_entity("Pet", vec![("breed", "str")], vec!["Named"]),
        ];
        spec.capabilities.push(cap);

        merge_inherited_fields(&mut spec);

        // Named should have id + name
        let named = &spec.capabilities[0].entities[1];
        assert_eq!(named.fields.len(), 2);
        assert_eq!(named.fields[0].name, "id");

        // Pet should have id + name + breed
        let pet = &spec.capabilities[0].entities[2];
        assert_eq!(pet.fields.len(), 3);
        assert_eq!(pet.fields[0].name, "id");
        assert_eq!(pet.fields[1].name, "name");
        assert_eq!(pet.fields[2].name, "breed");
    }

    #[test]
    fn test_field_override() {
        // Child redefines a parent field → should keep child's version
        let mut spec = ProjectSpec::new("test".to_string());

        let mut cap = Capability::new("models".to_string(), "models.py".to_string());
        cap.entities = vec![
            make_entity("Base", vec![("email", "str"), ("name", "str")], vec![]),
            make_entity("User", vec![("email", "EmailStr"), ("age", "int")], vec!["Base"]),
        ];
        spec.capabilities.push(cap);

        merge_inherited_fields(&mut spec);

        let user = &spec.capabilities[0].entities[1];
        assert_eq!(user.fields.len(), 3); // name (inherited) + email (own) + age (own)
        // email should be the child's version (EmailStr), not parent's (str)
        let email = user.fields.iter().find(|f| f.name == "email").unwrap();
        assert_eq!(email.field_type, "EmailStr");
    }

    #[test]
    fn test_no_bases_unchanged() {
        let mut spec = ProjectSpec::new("test".to_string());

        let mut cap = Capability::new("models".to_string(), "models.py".to_string());
        cap.entities = vec![
            make_entity("User", vec![("id", "int"), ("name", "str")], vec![]),
        ];
        spec.capabilities.push(cap);

        merge_inherited_fields(&mut spec);

        let user = &spec.capabilities[0].entities[0];
        assert_eq!(user.fields.len(), 2); // unchanged
    }

    #[test]
    fn test_cross_capability_inheritance() {
        // Parent entity in one capability, child in another (e.g., different files)
        let mut spec = ProjectSpec::new("test".to_string());

        let mut cap1 = Capability::new("base-model".to_string(), "base.py".to_string());
        cap1.entities = vec![make_entity("BaseEntity", vec![("id", "int"), ("created_at", "datetime")], vec![])];
        spec.capabilities.push(cap1);

        let mut cap2 = Capability::new("user-model".to_string(), "user.py".to_string());
        cap2.entities = vec![make_entity("User", vec![("name", "str")], vec!["BaseEntity"])];
        spec.capabilities.push(cap2);

        merge_inherited_fields(&mut spec);

        let user = &spec.capabilities[1].entities[0];
        assert_eq!(user.fields.len(), 3); // id + created_at + name
        assert_eq!(user.fields[0].name, "id");
    }
}
