use super::common::*;
use crate::context::ParsedFile;
use pecto_core::model::*;

/// Extract a Capability from a parsed file if it contains a JPA entity.
pub fn extract(file: &ParsedFile) -> Option<Capability> {
    let root = file.tree.root_node();
    let source = file.source.as_bytes();

    for i in 0..root.named_child_count() {
        let node = root.named_child(i).unwrap();
        if node.kind() == "class_declaration" {
            let annotations = collect_annotations(&node, source);

            let is_entity = annotations.iter().any(|a| a.name == "Entity");
            if !is_entity {
                continue;
            }

            let class_name = get_class_name(&node, source);
            let table_name = extract_table_name(&annotations, &class_name);

            let mut fields = Vec::new();
            if let Some(body) = node.child_by_field_name("body") {
                extract_fields(&body, source, &mut fields);
            }

            let bases = node
                .child_by_field_name("superclass")
                .map(|sc| vec![node_text(&sc, source)])
                .unwrap_or_default();

            let entity = Entity {
                name: class_name.clone(),
                table: table_name,
                fields,
                bases,
            };

            let capability_name = format!("{}-entity", to_kebab_case(&class_name));
            let mut capability = Capability::new(capability_name, file.path.clone());
            capability.entities.push(entity);
            return Some(capability);
        }
    }

    None
}

fn extract_table_name(annotations: &[AnnotationInfo], class_name: &str) -> String {
    for ann in annotations {
        if ann.name == "Table" {
            if let Some(name) = ann.arguments.get("name") {
                return name.clone();
            }
            if let Some(val) = &ann.value {
                return val.clone();
            }
        }
    }
    // Default: class name as-is (JPA convention is the class name)
    class_name.to_string()
}

fn extract_fields(body: &tree_sitter::Node, source: &[u8], fields: &mut Vec<EntityField>) {
    for i in 0..body.named_child_count() {
        let member = body.named_child(i).unwrap();
        if member.kind() != "field_declaration" {
            continue;
        }

        let annotations = collect_annotations(&member, source);

        // Skip static or transient fields
        let is_static = has_modifier(&member, source, "static");
        let is_transient = has_modifier(&member, source, "transient")
            || annotations.iter().any(|a| a.name == "Transient");
        if is_static || is_transient {
            continue;
        }

        let field_type = member
            .child_by_field_name("type")
            .map(|t| node_text(&t, source))
            .unwrap_or_default();

        // Extract field name from declarator
        let field_name = extract_field_name(&member, source);
        if field_name.is_empty() {
            continue;
        }

        let constraints = extract_field_constraints(&annotations);

        fields.push(EntityField {
            name: field_name,
            field_type,
            constraints,
        });
    }
}

fn extract_field_name(field_decl: &tree_sitter::Node, source: &[u8]) -> String {
    for i in 0..field_decl.named_child_count() {
        let child = field_decl.named_child(i).unwrap();
        if child.kind() == "variable_declarator"
            && let Some(name) = child.child_by_field_name("name")
        {
            return node_text(&name, source);
        }
    }
    String::new()
}

fn has_modifier(node: &tree_sitter::Node, source: &[u8], modifier: &str) -> bool {
    for i in 0..node.named_child_count() {
        let child = node.named_child(i).unwrap();
        if child.kind() == "modifiers" {
            // Check both named and unnamed children — keywords like
            // "static" and "transient" may be unnamed nodes
            for j in 0..child.child_count() {
                let m = child.child(j).unwrap();
                if m.kind() == modifier || node_text(&m, source) == modifier {
                    return true;
                }
            }
        }
    }
    false
}

/// Extract constraint strings from field annotations.
fn extract_field_constraints(annotations: &[AnnotationInfo]) -> Vec<String> {
    let mut constraints = Vec::new();

    for ann in annotations {
        match ann.name.as_str() {
            // JPA annotations
            "Id" => constraints.push("@Id".to_string()),
            "GeneratedValue" => {
                let strategy = ann
                    .arguments
                    .get("strategy")
                    .map(|s| format!("@GeneratedValue(strategy={})", s))
                    .unwrap_or_else(|| "@GeneratedValue".to_string());
                constraints.push(strategy);
            }
            "Column" => {
                let mut parts = Vec::new();
                if let Some(v) = ann.arguments.get("unique")
                    && v == "true"
                {
                    parts.push("unique=true".to_string());
                }
                if let Some(v) = ann.arguments.get("nullable")
                    && v == "false"
                {
                    parts.push("nullable=false".to_string());
                }
                if let Some(v) = ann.arguments.get("length") {
                    parts.push(format!("length={}", v));
                }
                if parts.is_empty() {
                    // Don't add bare @Column — not interesting
                } else {
                    constraints.push(format!("@Column({})", parts.join(", ")));
                }
            }

            // Relationship annotations
            "OneToMany" => constraints.push("@OneToMany".to_string()),
            "ManyToOne" => constraints.push("@ManyToOne".to_string()),
            "ManyToMany" => constraints.push("@ManyToMany".to_string()),
            "OneToOne" => constraints.push("@OneToOne".to_string()),

            // Bean validation annotations
            "NotNull" => constraints.push("@NotNull".to_string()),
            "NotBlank" => constraints.push("@NotBlank".to_string()),
            "NotEmpty" => constraints.push("@NotEmpty".to_string()),
            "Email" => constraints.push("@Email".to_string()),
            "Size" => {
                let mut parts = Vec::new();
                if let Some(v) = ann.arguments.get("min") {
                    parts.push(format!("min={}", v));
                }
                if let Some(v) = ann.arguments.get("max") {
                    parts.push(format!("max={}", v));
                }
                constraints.push(format!("@Size({})", parts.join(", ")));
            }
            "Min" => {
                let val = ann.value.as_deref().unwrap_or("?");
                constraints.push(format!("@Min({})", val));
            }
            "Max" => {
                let val = ann.value.as_deref().unwrap_or("?");
                constraints.push(format!("@Max({})", val));
            }
            "Pattern" => {
                let regexp = ann
                    .arguments
                    .get("regexp")
                    .map(|r| r.as_str())
                    .unwrap_or("?");
                constraints.push(format!("@Pattern(regexp={})", regexp));
            }
            "Positive" => constraints.push("@Positive".to_string()),
            "Negative" => constraints.push("@Negative".to_string()),
            "PositiveOrZero" => constraints.push("@PositiveOrZero".to_string()),
            "NegativeOrZero" => constraints.push("@NegativeOrZero".to_string()),
            "Past" => constraints.push("@Past".to_string()),
            "Future" => constraints.push("@Future".to_string()),
            "PastOrPresent" => constraints.push("@PastOrPresent".to_string()),
            "FutureOrPresent" => constraints.push("@FutureOrPresent".to_string()),

            _ => {}
        }
    }

    constraints
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ParsedFile;

    fn parse_file(source: &str, path: &str) -> ParsedFile {
        ParsedFile::parse(source.to_string(), path.to_string()).unwrap()
    }

    #[test]
    fn test_basic_entity() {
        let source = r#"
import javax.persistence.*;

@Entity
@Table(name = "users")
public class User {

    @Id
    @GeneratedValue(strategy = GenerationType.IDENTITY)
    private Long id;

    @Column(nullable = false, unique = true)
    @NotBlank
    @Email
    private String email;

    @Column(nullable = false)
    @Size(min = 2, max = 50)
    private String name;

    @Column(length = 255)
    private String bio;

    private int age;
}
"#;

        let file = parse_file(source, "src/User.java");
        let capability = extract(&file).unwrap();

        assert_eq!(capability.name, "user-entity");
        assert_eq!(capability.entities.len(), 1);

        let entity = &capability.entities[0];
        assert_eq!(entity.name, "User");
        assert_eq!(entity.table, "users");
        assert_eq!(entity.fields.len(), 5);

        // id field
        let id = &entity.fields[0];
        assert_eq!(id.name, "id");
        assert_eq!(id.field_type, "Long");
        assert!(id.constraints.contains(&"@Id".to_string()));
        assert!(
            id.constraints
                .iter()
                .any(|c| c.starts_with("@GeneratedValue"))
        );

        // email field
        let email = &entity.fields[1];
        assert_eq!(email.name, "email");
        assert!(email.constraints.contains(&"@NotBlank".to_string()));
        assert!(email.constraints.contains(&"@Email".to_string()));
        assert!(email.constraints.iter().any(|c| c.contains("unique=true")));

        // name field
        let name = &entity.fields[2];
        assert_eq!(name.name, "name");
        assert!(name.constraints.iter().any(|c| c.contains("@Size")));
    }

    #[test]
    fn test_entity_with_relationships() {
        let source = r#"
import javax.persistence.*;
import java.util.List;

@Entity
public class Owner {

    @Id
    @GeneratedValue
    private Long id;

    @NotBlank
    private String firstName;

    @OneToMany(mappedBy = "owner")
    private List<Pet> pets;

    @ManyToOne
    private City city;

    private static final long serialVersionUID = 1L;
    private transient String tempData;
}
"#;

        let file = parse_file(source, "src/Owner.java");
        let capability = extract(&file).unwrap();

        let entity = &capability.entities[0];
        assert_eq!(entity.name, "Owner");
        // Default table name = class name
        assert_eq!(entity.table, "Owner");
        // 4 fields: id, firstName, pets, city (static and transient excluded)
        assert_eq!(entity.fields.len(), 4);

        let pets = &entity.fields[2];
        assert_eq!(pets.name, "pets");
        assert!(pets.constraints.contains(&"@OneToMany".to_string()));

        let city = &entity.fields[3];
        assert_eq!(city.name, "city");
        assert!(city.constraints.contains(&"@ManyToOne".to_string()));
    }

    #[test]
    fn test_non_entity_class_returns_none() {
        let source = r#"
public class UserService {
    private final UserRepository repo;
    public User findById(Long id) { return repo.findById(id).orElse(null); }
}
"#;

        let file = parse_file(source, "src/UserService.java");
        assert!(extract(&file).is_none());
    }
}
