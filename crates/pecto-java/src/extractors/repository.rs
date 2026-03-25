use super::common::*;
use crate::context::ParsedFile;
use pecto_core::model::*;

/// Standard CRUD operations implied by JpaRepository/CrudRepository.
const CRUD_OPS: &[(&str, &str)] = &[
    ("save", "Save or update an entity"),
    ("findById", "Find entity by ID"),
    ("findAll", "Find all entities"),
    ("deleteById", "Delete entity by ID"),
    ("count", "Count all entities"),
    ("existsById", "Check if entity exists by ID"),
];

/// Extract a Capability from a parsed file if it contains a Spring Data repository.
pub fn extract(file: &ParsedFile) -> Option<Capability> {
    let root = file.tree.root_node();
    let source = file.source.as_bytes();

    for i in 0..root.named_child_count() {
        let node = root.named_child(i).unwrap();
        if node.kind() != "interface_declaration" {
            continue;
        }

        // Check if it extends a known repository interface
        let (entity_type, _id_type) = match extract_repository_generics(&node, source) {
            Some(types) => types,
            None => continue,
        };

        let interface_name = get_class_name(&node, source);
        let capability_name = format!("{}-repository", to_kebab_case(&entity_type));
        let mut capability = Capability::new(capability_name, file.path.clone());

        // Add standard CRUD operations
        for &(name, _description) in CRUD_OPS {
            capability.operations.push(Operation {
                name: name.to_string(),
                source_method: format!("{}#{}", interface_name, name),
                input: None,
                behaviors: vec![],
                transaction: None,
            });
        }

        // Extract custom query methods
        if let Some(body) = node.child_by_field_name("body") {
            extract_custom_methods(&body, source, &interface_name, &mut capability);
        }

        return Some(capability);
    }

    None
}

/// Check if an interface extends JpaRepository, CrudRepository, etc. and extract generic types.
fn extract_repository_generics(
    node: &tree_sitter::Node,
    source: &[u8],
) -> Option<(String, String)> {
    // Look for superinterfaces: extends JpaRepository<Entity, Long>
    for i in 0..node.named_child_count() {
        let child = node.named_child(i).unwrap();
        if child.kind() == "extends_interfaces" || child.kind() == "super_interfaces" {
            let text = node_text(&child, source);
            if is_repository_type(&text) {
                return extract_generic_params(&text);
            }
        }
        // Also check type_list under extends_interfaces
        if child.kind() == "type_list" {
            for j in 0..child.named_child_count() {
                let type_node = child.named_child(j).unwrap();
                let text = node_text(&type_node, source);
                if is_repository_type(&text) {
                    return extract_generic_params(&text);
                }
            }
        }
    }
    None
}

fn is_repository_type(text: &str) -> bool {
    text.contains("JpaRepository")
        || text.contains("CrudRepository")
        || text.contains("PagingAndSortingRepository")
        || text.contains("ReactiveCrudRepository")
}

fn extract_generic_params(text: &str) -> Option<(String, String)> {
    let start = text.find('<')?;
    let end = text.rfind('>')?;
    let inner = &text[start + 1..end];
    let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
    if parts.len() >= 2 {
        Some((parts[0].to_string(), parts[1].to_string()))
    } else {
        None
    }
}

fn extract_custom_methods(
    body: &tree_sitter::Node,
    source: &[u8],
    interface_name: &str,
    capability: &mut Capability,
) {
    for i in 0..body.named_child_count() {
        let member = body.named_child(i).unwrap();
        if member.kind() != "method_declaration" {
            continue;
        }

        let method_name = member
            .child_by_field_name("name")
            .map(|n| node_text(&n, source))
            .unwrap_or_default();

        if method_name.is_empty() {
            continue;
        }

        let annotations = collect_annotations(&member, source);

        // Check for @Query annotation
        let query = annotations
            .iter()
            .find(|a| a.name == "Query")
            .and_then(|a| a.value.clone());

        let description = if let Some(ref q) = query {
            q.clone()
        } else {
            describe_query_method(&method_name)
        };

        capability.operations.push(Operation {
            name: description,
            source_method: format!("{}#{}", interface_name, method_name),
            input: None,
            behaviors: vec![],
            transaction: None,
        });
    }
}

/// Parse Spring Data query method names into human-readable descriptions.
/// e.g., "findByEmailAndStatus" -> "Find by email and status"
fn describe_query_method(name: &str) -> String {
    if let Some(rest) = name.strip_prefix("findBy") {
        format!("Find by {}", split_camel_case_query(rest))
    } else if let Some(rest) = name.strip_prefix("countBy") {
        format!("Count by {}", split_camel_case_query(rest))
    } else if let Some(rest) = name.strip_prefix("deleteBy") {
        format!("Delete by {}", split_camel_case_query(rest))
    } else if let Some(rest) = name.strip_prefix("existsBy") {
        format!("Exists by {}", split_camel_case_query(rest))
    } else {
        name.to_string()
    }
}

/// Split camelCase query method parts: "EmailAndStatus" -> "email and status"
fn split_camel_case_query(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.char_indices() {
        if c.is_uppercase() && i > 0 {
            // Check if this is a keyword like And, Or, Between, etc.
            let remaining = &s[i..];
            if remaining.starts_with("And") {
                result.push_str(" and ");
                continue;
            } else if remaining.starts_with("Or") {
                result.push_str(" or ");
                continue;
            } else if remaining.starts_with("OrderBy") {
                result.push_str(" order by ");
                // Skip "OrderBy" prefix
                continue;
            } else if remaining.starts_with("Between")
                || remaining.starts_with("LessThan")
                || remaining.starts_with("GreaterThan")
                || remaining.starts_with("After")
                || remaining.starts_with("Before")
                || remaining.starts_with("Containing")
                || remaining.starts_with("StartingWith")
                || remaining.starts_with("EndingWith")
                || remaining.starts_with("Not")
                || remaining.starts_with("In")
                || remaining.starts_with("IsNull")
                || remaining.starts_with("IsNotNull")
            {
                result.push(' ');
                result.push(c.to_ascii_lowercase());
                continue;
            }
        }
        result.push(c.to_ascii_lowercase());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ParsedFile;

    fn parse_file(source: &str, path: &str) -> ParsedFile {
        ParsedFile::parse(source.to_string(), path.to_string()).unwrap()
    }

    #[test]
    fn test_basic_repository() {
        let source = r#"
import org.springframework.data.jpa.repository.JpaRepository;

public interface UserRepository extends JpaRepository<User, Long> {
}
"#;

        let file = parse_file(source, "src/UserRepository.java");
        let capability = extract(&file).unwrap();

        assert_eq!(capability.name, "user-repository");
        // Standard CRUD ops
        assert!(capability.operations.len() >= 6);
        assert!(capability.operations.iter().any(|o| o.name == "save"));
        assert!(capability.operations.iter().any(|o| o.name == "findById"));
        assert!(capability.operations.iter().any(|o| o.name == "deleteById"));
    }

    #[test]
    fn test_repository_with_custom_queries() {
        let source = r#"
import org.springframework.data.jpa.repository.JpaRepository;
import org.springframework.data.jpa.repository.Query;

public interface UserRepository extends JpaRepository<User, Long> {

    List<User> findByEmail(String email);

    List<User> findByStatusAndCreatedDateAfter(String status, LocalDate date);

    @Query("SELECT u FROM User u WHERE u.active = true")
    List<User> findAllActive();

    long countByStatus(String status);
}
"#;

        let file = parse_file(source, "src/UserRepository.java");
        let capability = extract(&file).unwrap();

        // 6 CRUD + 4 custom = 10
        assert_eq!(capability.operations.len(), 10);

        // Custom query methods
        let custom_ops: Vec<&Operation> = capability
            .operations
            .iter()
            .filter(|o| o.source_method.contains("#find") || o.source_method.contains("#count"))
            .collect();

        // findByEmail
        assert!(
            custom_ops
                .iter()
                .any(|o| o.name.contains("email") && o.source_method.contains("findByEmail"))
        );

        // findByStatusAndCreatedDateAfter
        assert!(
            custom_ops
                .iter()
                .any(|o| o.name.contains("status") && o.name.contains("and"))
        );

        // @Query method
        assert!(
            capability
                .operations
                .iter()
                .any(|o| o.name.contains("SELECT"))
        );

        // countByStatus
        assert!(
            custom_ops
                .iter()
                .any(|o| o.name.contains("status") && o.source_method.contains("countByStatus"))
        );
    }

    #[test]
    fn test_non_repository_interface() {
        let source = r#"
public interface UserService {
    User findById(Long id);
}
"#;

        let file = parse_file(source, "src/UserService.java");
        assert!(extract(&file).is_none());
    }

    #[test]
    fn test_crud_repository() {
        let source = r#"
import org.springframework.data.repository.CrudRepository;

public interface OrderRepository extends CrudRepository<Order, String> {
    List<Order> findByCustomerIdOrderByCreatedDateDesc(String customerId);
}
"#;

        let file = parse_file(source, "src/OrderRepository.java");
        let capability = extract(&file).unwrap();

        assert_eq!(capability.name, "order-repository");
        // 6 CRUD + 1 custom
        assert_eq!(capability.operations.len(), 7);
    }
}
