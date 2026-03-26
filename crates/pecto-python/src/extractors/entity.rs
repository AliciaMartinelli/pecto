use super::common::*;
use crate::context::ParsedFile;
use pecto_core::model::*;

/// Extract entities from Python models: SQLAlchemy, Django ORM, Pydantic.
pub fn extract(file: &ParsedFile) -> Option<Capability> {
    let root = file.tree.root_node();
    let source = file.source.as_bytes();
    let full_text = &file.source;

    // Quick check: does this file have model-like patterns?
    if !full_text.contains("Column(")
        && !full_text.contains("models.Model")
        && !full_text.contains("BaseModel")
        && !full_text.contains("Base)")
        && !full_text.contains("DeclarativeBase")
    {
        return None;
    }

    let mut entities = Vec::new();

    for i in 0..root.named_child_count() {
        let node = root.named_child(i).unwrap();

        let class_node = if node.kind() == "class_definition" {
            node
        } else if node.kind() == "decorated_definition" {
            match get_inner_definition(&node) {
                Some(n) if n.kind() == "class_definition" => n,
                _ => continue,
            }
        } else {
            continue;
        };

        let class_name = get_def_name(&class_node, source);
        let bases = get_class_bases(&class_node, source);

        // SQLAlchemy: class User(Base) or class User(DeclarativeBase)
        if bases
            .iter()
            .any(|b| b == "Base" || b.contains("DeclarativeBase"))
            && let Some(entity) = extract_sqlalchemy_entity(&class_node, source, &class_name)
        {
            entities.push(entity);
        }
        // Django: class User(models.Model)
        else if bases
            .iter()
            .any(|b| b == "models.Model" || b.starts_with("models."))
            && let Some(entity) = extract_django_model(&class_node, source, &class_name)
        {
            entities.push(entity);
        }
        // Pydantic: class UserCreate(BaseModel)
        else if bases
            .iter()
            .any(|b| b == "BaseModel" || b.contains("BaseModel"))
            && let Some(entity) = extract_pydantic_model(&class_node, source, &class_name)
        {
            entities.push(entity);
        }
    }

    if entities.is_empty() {
        return None;
    }

    let file_stem = file
        .path
        .rsplit('/')
        .next()
        .unwrap_or(&file.path)
        .trim_end_matches(".py");
    let capability_name = format!("{}-model", to_kebab_case(file_stem));

    let mut capability = Capability::new(capability_name, file.path.clone());
    capability.entities = entities;
    Some(capability)
}

fn get_class_bases(class_node: &tree_sitter::Node, source: &[u8]) -> Vec<String> {
    let mut bases = Vec::new();
    if let Some(arg_list) = class_node.child_by_field_name("superclasses") {
        for i in 0..arg_list.named_child_count() {
            let arg = arg_list.named_child(i).unwrap();
            bases.push(node_text(&arg, source));
        }
    }
    bases
}

/// Extract SQLAlchemy model: Column(String), Column(Integer), relationship()
fn extract_sqlalchemy_entity(
    class_node: &tree_sitter::Node,
    source: &[u8],
    class_name: &str,
) -> Option<Entity> {
    let body = class_node.child_by_field_name("body")?;
    let mut fields = Vec::new();
    let mut table_name = class_name.to_lowercase();

    for i in 0..body.named_child_count() {
        let stmt = body.named_child(i).unwrap();
        if stmt.kind() != "expression_statement" {
            continue;
        }

        let text = node_text(&stmt, source);

        // __tablename__ = "users"
        if text.contains("__tablename__") {
            if let Some(val) = extract_assignment_string(&text) {
                table_name = val;
            }
            continue;
        }

        // field = Column(Type, ...)
        if (text.contains("Column(") || text.contains("relationship("))
            && let Some(field) = parse_sqlalchemy_field(&text)
        {
            fields.push(field);
        }
    }

    Some(Entity {
        name: class_name.to_string(),
        table: table_name,
        fields,
    })
}

fn parse_sqlalchemy_field(text: &str) -> Option<EntityField> {
    let parts: Vec<&str> = text.splitn(2, '=').collect();
    if parts.len() != 2 {
        return None;
    }

    let name = parts[0].trim().to_string();
    let rhs = parts[1].trim();

    if rhs.starts_with("Column(") {
        let inner = &rhs[7..rhs.rfind(')')?];
        let args: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
        let field_type = args.first().unwrap_or(&"").to_string();

        let mut constraints = Vec::new();
        for arg in &args[1..] {
            if arg.contains("primary_key=True") {
                constraints.push("primary_key".to_string());
            }
            if arg.contains("nullable=False") {
                constraints.push("required".to_string());
            }
            if arg.contains("unique=True") {
                constraints.push("unique".to_string());
            }
            if arg.contains("index=True") {
                constraints.push("indexed".to_string());
            }
        }

        Some(EntityField {
            name,
            field_type,
            constraints,
        })
    } else if rhs.starts_with("relationship(") {
        let inner = &rhs[13..rhs.rfind(')')?];
        let target = inner
            .split(',')
            .next()?
            .trim()
            .trim_matches('"')
            .trim_matches('\'');
        Some(EntityField {
            name,
            field_type: format!("relationship({})", target),
            constraints: vec!["relationship".to_string()],
        })
    } else {
        None
    }
}

/// Extract Django model: CharField, IntegerField, ForeignKey, etc.
fn extract_django_model(
    class_node: &tree_sitter::Node,
    source: &[u8],
    class_name: &str,
) -> Option<Entity> {
    let body = class_node.child_by_field_name("body")?;
    let mut fields = Vec::new();
    let table_name = class_name.to_lowercase();

    for i in 0..body.named_child_count() {
        let stmt = body.named_child(i).unwrap();
        if stmt.kind() != "expression_statement" {
            continue;
        }

        let text = node_text(&stmt, source);

        // field = models.CharField(...) or field = CharField(...)
        if (text.contains("Field(")
            || text.contains("ForeignKey(")
            || text.contains("ManyToManyField(")
            || text.contains("OneToOneField("))
            && let Some(field) = parse_django_field(&text)
        {
            fields.push(field);
        }
    }

    Some(Entity {
        name: class_name.to_string(),
        table: table_name,
        fields,
    })
}

fn parse_django_field(text: &str) -> Option<EntityField> {
    let parts: Vec<&str> = text.splitn(2, '=').collect();
    if parts.len() != 2 {
        return None;
    }

    let name = parts[0].trim().to_string();
    let rhs = parts[1].trim();

    // Extract field type: models.CharField or CharField
    let field_type = rhs.split('(').next()?.trim().replace("models.", "");

    let mut constraints = Vec::new();
    if rhs.contains("primary_key=True") {
        constraints.push("primary_key".to_string());
    }
    if rhs.contains("blank=False")
        || rhs.contains("null=False")
        || !rhs.contains("blank=True") && !rhs.contains("null=True")
    {
        constraints.push("required".to_string());
    }
    if rhs.contains("unique=True") {
        constraints.push("unique".to_string());
    }
    if rhs.contains("max_length=")
        && let Some(ml) = extract_kwarg_value(rhs, "max_length")
    {
        constraints.push(format!("max_length={}", ml));
    }
    if field_type.contains("ForeignKey") || field_type.contains("OneToOne") {
        constraints.push("relationship".to_string());
    }
    if field_type.contains("ManyToMany") {
        constraints.push("many_to_many".to_string());
    }

    Some(EntityField {
        name,
        field_type,
        constraints,
    })
}

/// Extract Pydantic model fields from type annotations.
fn extract_pydantic_model(
    class_node: &tree_sitter::Node,
    source: &[u8],
    class_name: &str,
) -> Option<Entity> {
    let body = class_node.child_by_field_name("body")?;
    let mut fields = Vec::new();

    for i in 0..body.named_child_count() {
        let stmt = body.named_child(i).unwrap();

        let text = node_text(&stmt, source);

        // name: str or name: str = Field(...)
        if stmt.kind() == "expression_statement" && text.contains(':') {
            let parts: Vec<&str> = text.splitn(2, ':').collect();
            if parts.len() == 2 {
                let name = parts[0].trim().to_string();
                let type_and_default = parts[1].trim();
                let field_type = type_and_default
                    .split('=')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string();

                if name.starts_with('_') || name == "model_config" || name == "Config" {
                    continue;
                }

                let mut constraints = Vec::new();
                if text.contains("Field(") {
                    if let Some(v) = extract_kwarg_value(&text, "min_length") {
                        constraints.push(format!("min_length={}", v));
                    }
                    if let Some(v) = extract_kwarg_value(&text, "max_length") {
                        constraints.push(format!("max_length={}", v));
                    }
                    if text.contains("gt=") || text.contains("ge=") {
                        constraints.push("min_value".to_string());
                    }
                    if text.contains("lt=") || text.contains("le=") {
                        constraints.push("max_value".to_string());
                    }
                }

                if !field_type.starts_with("Optional") && !text.contains("= None") {
                    constraints.push("required".to_string());
                }

                fields.push(EntityField {
                    name,
                    field_type,
                    constraints,
                });
            }
        }
    }

    Some(Entity {
        name: class_name.to_string(),
        table: class_name.to_lowercase(),
        fields,
    })
}

fn extract_assignment_string(text: &str) -> Option<String> {
    let after_eq = text.split('=').nth(1)?.trim();
    Some(clean_string_literal(after_eq))
}

fn extract_kwarg_value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let pattern = format!("{}=", key);
    let start = text.find(&pattern)? + pattern.len();
    let remaining = &text[start..];
    let end = remaining.find([',', ')'])?;
    Some(remaining[..end].trim())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ParsedFile;

    fn parse_file(source: &str, path: &str) -> ParsedFile {
        ParsedFile::parse(source.to_string(), path.to_string()).unwrap()
    }

    #[test]
    fn test_sqlalchemy_model() {
        let source = r#"
from sqlalchemy import Column, Integer, String, ForeignKey
from sqlalchemy.orm import relationship

class User(Base):
    __tablename__ = "users"

    id = Column(Integer, primary_key=True)
    name = Column(String, nullable=False)
    email = Column(String, unique=True)
    posts = relationship("Post")
"#;

        let file = parse_file(source, "models/user.py");
        let capability = extract(&file).unwrap();

        let entity = &capability.entities[0];
        assert_eq!(entity.name, "User");
        assert_eq!(entity.table, "users");
        assert_eq!(entity.fields.len(), 4);
        assert!(
            entity.fields[0]
                .constraints
                .contains(&"primary_key".to_string())
        );
        assert!(entity.fields[2].constraints.contains(&"unique".to_string()));
    }

    #[test]
    fn test_django_model() {
        let source = r#"
from django.db import models

class Article(models.Model):
    title = models.CharField(max_length=200)
    content = models.TextField()
    author = models.ForeignKey("User", on_delete=models.CASCADE)
    tags = models.ManyToManyField("Tag")
"#;

        let file = parse_file(source, "models.py");
        let capability = extract(&file).unwrap();

        let entity = &capability.entities[0];
        assert_eq!(entity.name, "Article");
        assert_eq!(entity.fields.len(), 4);
        assert!(
            entity.fields[0]
                .constraints
                .iter()
                .any(|c| c.contains("max_length"))
        );
        assert!(
            entity.fields[2]
                .constraints
                .contains(&"relationship".to_string())
        );
    }

    #[test]
    fn test_pydantic_model() {
        let source = r#"
from pydantic import BaseModel, Field

class UserCreate(BaseModel):
    name: str = Field(min_length=2, max_length=50)
    email: str
    age: int = Field(gt=0, lt=150)
    bio: Optional[str] = None
"#;

        let file = parse_file(source, "schemas/user.py");
        let capability = extract(&file).unwrap();

        let entity = &capability.entities[0];
        assert_eq!(entity.name, "UserCreate");
        assert_eq!(entity.fields.len(), 4);
        assert!(
            entity.fields[0]
                .constraints
                .iter()
                .any(|c| c.contains("min_length"))
        );
        // bio is Optional with default None → not required
        assert!(
            !entity.fields[3]
                .constraints
                .contains(&"required".to_string())
        );
    }

    #[test]
    fn test_no_model() {
        let source = r#"
class Helper:
    def do_something(self):
        pass
"#;
        let file = parse_file(source, "utils.py");
        assert!(extract(&file).is_none());
    }
}
