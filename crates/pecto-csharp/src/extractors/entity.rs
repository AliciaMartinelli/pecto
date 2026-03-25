use super::common::*;
use crate::context::{AnalysisContext, ParsedFile};
use pecto_core::model::*;
use tree_sitter::Node;

/// Extract entities from a parsed file.
/// Detects: classes with [Table] attribute, or classes referenced via DbSet<T> in a DbContext.
pub fn extract(file: &ParsedFile, ctx: &AnalysisContext) -> Option<Capability> {
    let root = file.tree.root_node();
    let source = file.source.as_bytes();

    let mut result = None;

    for_each_class(&root, source, &mut |node, src| {
        if result.is_some() {
            return;
        }

        let attrs = collect_attributes(node, src);
        let class_name = get_class_name(node, src);

        // Path A: class has [Table] attribute → it's an entity
        if attrs.iter().any(|a| a.name == "Table") {
            let table_name = extract_table_name(&attrs, &class_name);
            let fields = extract_properties(node, src);
            let entity = Entity {
                name: class_name.clone(),
                table: table_name,
                fields,
            };
            let cap_name = format!("{}-entity", to_kebab_case(&class_name));
            let mut cap = Capability::new(cap_name, file.path.clone());
            cap.entities.push(entity);
            result = Some(cap);
            return;
        }

        // Path B: class inherits from DbContext → scan for DbSet<T> properties
        if is_db_context(node, src) {
            let dbset_types = extract_dbset_types(node, src);
            if !dbset_types.is_empty() {
                let cap_name = to_kebab_case(&class_name).to_string();
                let mut cap = Capability::new(cap_name, file.path.clone());

                for entity_type in &dbset_types {
                    if let Some(entity) = resolve_entity(entity_type, ctx) {
                        cap.entities.push(entity);
                    } else {
                        // Entity class not found in context — create stub
                        cap.entities.push(Entity {
                            name: entity_type.clone(),
                            table: entity_type.clone(),
                            fields: Vec::new(),
                        });
                    }
                }

                result = Some(cap);
            }
        }
    });

    // Also check for entity classes without [Table] but with [Key] on a property
    if result.is_none() {
        for_each_class(&root, source, &mut |node, src| {
            if result.is_some() {
                return;
            }

            let class_name = get_class_name(node, src);
            if let Some(body) = node.child_by_field_name("body")
                && has_key_property(&body, src)
            {
                let fields = extract_properties(node, src);
                let entity = Entity {
                    name: class_name.clone(),
                    table: class_name.clone(),
                    fields,
                };
                let cap_name = format!("{}-entity", to_kebab_case(&class_name));
                let mut cap = Capability::new(cap_name, file.path.clone());
                cap.entities.push(entity);
                result = Some(cap);
            }
        });
    }

    result
}

fn extract_table_name(attrs: &[AttributeInfo], class_name: &str) -> String {
    for attr in attrs {
        if attr.name == "Table"
            && let Some(val) = &attr.value
        {
            return val.clone();
        }
    }
    class_name.to_string()
}

fn is_db_context(node: &Node, source: &[u8]) -> bool {
    // Check base_list for DbContext inheritance
    for i in 0..node.named_child_count() {
        let child = node.named_child(i).unwrap();
        if child.kind() == "base_list" {
            let text = node_text(&child, source);
            if text.contains("DbContext") {
                return true;
            }
        }
    }
    false
}

/// Extract types from DbSet<T> properties in a DbContext class.
fn extract_dbset_types(node: &Node, source: &[u8]) -> Vec<String> {
    let mut types = Vec::new();
    let Some(body) = node.child_by_field_name("body") else {
        return types;
    };

    for i in 0..body.named_child_count() {
        let member = body.named_child(i).unwrap();
        if member.kind() == "property_declaration" {
            let type_text = member
                .child_by_field_name("type")
                .map(|t| node_text(&t, source))
                .unwrap_or_default();

            if let Some(entity_type) = extract_dbset_generic(&type_text) {
                types.push(entity_type);
            }
        }
    }

    types
}

/// Extract T from DbSet<T>.
fn extract_dbset_generic(type_text: &str) -> Option<String> {
    let trimmed = type_text.trim();
    if trimmed.starts_with("DbSet<") && trimmed.ends_with('>') {
        Some(trimmed[6..trimmed.len() - 1].trim().to_string())
    } else {
        None
    }
}

/// Check if any property has [Key] attribute.
fn has_key_property(body: &Node, source: &[u8]) -> bool {
    for i in 0..body.named_child_count() {
        let member = body.named_child(i).unwrap();
        if member.kind() == "property_declaration" {
            let attrs = collect_attributes(&member, source);
            if attrs.iter().any(|a| a.name == "Key") {
                return true;
            }
        }
    }
    false
}

/// Resolve an entity type from the AnalysisContext and extract its properties.
fn resolve_entity(type_name: &str, ctx: &AnalysisContext) -> Option<Entity> {
    let file = ctx.find_class_by_name(type_name)?;
    let root = file.tree.root_node();
    let source = file.source.as_bytes();

    let mut entity = None;
    for_each_class(&root, source, &mut |node, src| {
        if entity.is_some() || get_class_name(node, src) != type_name {
            return;
        }

        let attrs = collect_attributes(node, src);
        let table_name = extract_table_name(&attrs, type_name);
        let fields = extract_properties(node, src);

        entity = Some(Entity {
            name: type_name.to_string(),
            table: table_name,
            fields,
        });
    });

    entity
}

/// Extract properties from a class as EntityFields.
fn extract_properties(class_node: &Node, source: &[u8]) -> Vec<EntityField> {
    let mut fields = Vec::new();
    let Some(body) = class_node.child_by_field_name("body") else {
        return fields;
    };

    for i in 0..body.named_child_count() {
        let member = body.named_child(i).unwrap();
        if member.kind() != "property_declaration" {
            continue;
        }

        let attrs = collect_attributes(&member, source);

        // Skip [NotMapped] properties
        if attrs.iter().any(|a| a.name == "NotMapped") {
            continue;
        }

        let prop_type = member
            .child_by_field_name("type")
            .map(|t| node_text(&t, source))
            .unwrap_or_default();
        let prop_name = member
            .child_by_field_name("name")
            .map(|n| node_text(&n, source))
            .unwrap_or_default();

        if prop_name.is_empty() {
            continue;
        }

        let constraints = extract_property_constraints(&attrs);

        fields.push(EntityField {
            name: prop_name,
            field_type: prop_type,
            constraints,
        });
    }

    fields
}

fn extract_property_constraints(attrs: &[AttributeInfo]) -> Vec<String> {
    let mut constraints = Vec::new();
    for attr in attrs {
        match attr.name.as_str() {
            "Key" => constraints.push("[Key]".to_string()),
            "Required" => constraints.push("[Required]".to_string()),
            "MaxLength" => {
                let val = attr.value.as_deref().unwrap_or("?");
                constraints.push(format!("[MaxLength({})]", val));
            }
            "MinLength" => {
                let val = attr.value.as_deref().unwrap_or("?");
                constraints.push(format!("[MinLength({})]", val));
            }
            "StringLength" => {
                let val = attr.value.as_deref().unwrap_or("?");
                constraints.push(format!("[StringLength({})]", val));
            }
            "Column" => {
                if let Some(val) = &attr.value {
                    constraints.push(format!("[Column({})]", val));
                }
            }
            "ForeignKey" => {
                let val = attr.value.as_deref().unwrap_or("?");
                constraints.push(format!("[ForeignKey({})]", val));
            }
            "InverseProperty" => {
                let val = attr.value.as_deref().unwrap_or("?");
                constraints.push(format!("[InverseProperty({})]", val));
            }
            "DatabaseGenerated" => {
                let val = attr.value.as_deref().unwrap_or("Identity");
                constraints.push(format!("[DatabaseGenerated({})]", val));
            }
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

    fn empty_ctx() -> AnalysisContext {
        AnalysisContext::new(vec![])
    }

    #[test]
    fn test_entity_with_table_attribute() {
        let source = r#"
using System.ComponentModel.DataAnnotations;
using System.ComponentModel.DataAnnotations.Schema;

namespace MyApp.Models;

[Table("users")]
public class User
{
    [Key]
    [DatabaseGenerated(DatabaseGeneratedOption.Identity)]
    public int Id { get; set; }

    [Required]
    [MaxLength(100)]
    public string Name { get; set; }

    [Required]
    [MaxLength(255)]
    public string Email { get; set; }

    public string? Bio { get; set; }

    [NotMapped]
    public string FullDisplay { get; set; }
}
"#;

        let file = parse_file(source, "Models/User.cs");
        let capability = extract(&file, &empty_ctx()).unwrap();

        assert_eq!(capability.entities.len(), 1);
        let entity = &capability.entities[0];
        assert_eq!(entity.name, "User");
        assert_eq!(entity.table, "users");
        // 4 fields (FullDisplay is NotMapped)
        assert_eq!(entity.fields.len(), 4);

        let id = &entity.fields[0];
        assert_eq!(id.name, "Id");
        assert!(id.constraints.contains(&"[Key]".to_string()));

        let name = &entity.fields[1];
        assert!(name.constraints.contains(&"[Required]".to_string()));
        assert!(name.constraints.iter().any(|c| c.contains("MaxLength")));
    }

    #[test]
    fn test_dbcontext_dbset_resolution() {
        let entity_source = r#"
namespace MyApp.Models;

[Table("orders")]
public class Order
{
    [Key]
    public int Id { get; set; }

    [Required]
    public string Status { get; set; }

    public decimal Total { get; set; }
}
"#;

        let dbcontext_source = r#"
using Microsoft.EntityFrameworkCore;

namespace MyApp.Data;

public class AppDbContext : DbContext
{
    public DbSet<Order> Orders { get; set; }
    public DbSet<Product> Products { get; set; }
}
"#;

        let entity_file = parse_file(entity_source, "Models/Order.cs");
        let dbcontext_file = parse_file(dbcontext_source, "Data/AppDbContext.cs");

        let ctx = AnalysisContext::new(vec![entity_file]);
        let capability = extract(&dbcontext_file, &ctx).unwrap();

        assert_eq!(capability.name, "app-db-context");
        assert_eq!(capability.entities.len(), 2);

        // Order should be resolved with fields
        let order = capability
            .entities
            .iter()
            .find(|e| e.name == "Order")
            .unwrap();
        assert_eq!(order.table, "orders");
        assert!(!order.fields.is_empty());

        // Product is not in context — stub with no fields
        let product = capability
            .entities
            .iter()
            .find(|e| e.name == "Product")
            .unwrap();
        assert!(product.fields.is_empty());
    }

    #[test]
    fn test_entity_with_key_property() {
        let source = r#"
namespace MyApp.Models;

public class Category
{
    [Key]
    public int CategoryId { get; set; }
    public string Name { get; set; }
    public List<Product> Products { get; set; }
}
"#;

        let file = parse_file(source, "Models/Category.cs");
        let capability = extract(&file, &empty_ctx()).unwrap();

        assert_eq!(capability.entities.len(), 1);
        let entity = &capability.entities[0];
        assert_eq!(entity.name, "Category");
        assert_eq!(entity.fields.len(), 3);
    }

    #[test]
    fn test_non_entity_class() {
        let source = r#"
namespace MyApp.Services;

public class OrderService
{
    public Order FindById(int id) { return null; }
}
"#;

        let file = parse_file(source, "Services/OrderService.cs");
        assert!(extract(&file, &empty_ctx()).is_none());
    }
}
