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
        && !full_text.contains("mapped_column(")
        && !full_text.contains("models.Model")
        && !full_text.contains("BaseModel")
        && !full_text.contains("SQLModel")
        && !full_text.contains("Base)")
        && !full_text.contains("DeclarativeBase")
    {
        return None;
    }

    let mut entities = Vec::new();

    // First pass: collect all model class names for same-file inheritance resolution
    let mut known_model_classes: Vec<String> = Vec::new();

    // Pre-scan to identify known model base classes in this file
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

        let name = get_def_name(&class_node, source);
        let bases = get_class_bases(&class_node, source);

        // Mark classes that directly extend known model frameworks
        let is_direct_model = bases.iter().any(|b| {
            b == "Base"
                || b.contains("DeclarativeBase")
                || b == "models.Model"
                || b.starts_with("models.")
                || b == "BaseModel"
                || b == "SQLModel"
        });

        if is_direct_model || has_table_kwarg(&class_node, source) {
            known_model_classes.push(name);
        }
    }

    // Second pass: add classes that inherit from already-known model classes
    // This handles chains like: SQLModel -> UserBase -> UserCreate
    let mut changed = true;
    while changed {
        changed = false;
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

            let name = get_def_name(&class_node, source);
            if known_model_classes.contains(&name) {
                continue;
            }
            let bases = get_class_bases(&class_node, source);
            if bases
                .iter()
                .any(|b| known_model_classes.iter().any(|k| k == b))
            {
                known_model_classes.push(name);
                changed = true;
            }
        }
    }

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

        // Skip base class definitions themselves (e.g. class Base(DeclarativeBase))
        if is_base_class_definition(&class_name) {
            continue;
        }

        let bases = get_class_bases(&class_node, source);

        // Check if class has table=True kwarg (SQLModel database model)
        let has_table_true = has_table_kwarg(&class_node, source);

        // Filter bases to only real class names (exclude keyword args like table=True)
        let class_bases: Vec<String> = bases.iter().filter(|b| !b.contains('=')).cloned().collect();

        // SQLAlchemy: class User(Base) or class User(DeclarativeBase)
        if bases
            .iter()
            .any(|b| b == "Base" || b.contains("DeclarativeBase"))
            && let Some(mut entity) = extract_sqlalchemy_entity(&class_node, source, &class_name)
        {
            entity.bases = class_bases;
            entities.push(entity);
        }
        // SQLModel with table=True: database-backed model (e.g. class User(UserBase, table=True))
        else if has_table_true
            && full_text.contains("SQLModel")
            && let Some(mut entity) = extract_sqlmodel_entity(&class_node, source, &class_name)
        {
            entity.bases = class_bases;
            entities.push(entity);
        }
        // Django: class User(models.Model)
        else if bases
            .iter()
            .any(|b| b == "models.Model" || b.starts_with("models."))
            && let Some(mut entity) = extract_django_model(&class_node, source, &class_name)
        {
            entity.bases = class_bases;
            entities.push(entity);
        }
        // Pydantic/SQLModel (non-table): direct base or inherits from known model in same file
        else if bases.iter().any(|b| {
            b == "BaseModel" || b == "SQLModel" || known_model_classes.iter().any(|k| k == b)
        }) && let Some(mut entity) =
            extract_pydantic_model(&class_node, source, &class_name)
        {
            entity.bases = class_bases;
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

/// Check if a class definition has `table=True` in its keyword arguments.
fn has_table_kwarg(class_node: &tree_sitter::Node, source: &[u8]) -> bool {
    if let Some(arg_list) = class_node.child_by_field_name("superclasses") {
        for i in 0..arg_list.named_child_count() {
            let arg = arg_list.named_child(i).unwrap();
            if arg.kind() == "keyword_argument" {
                let text = node_text(&arg, source);
                if text.contains("table") && text.contains("True") {
                    return true;
                }
            }
        }
    }
    false
}

/// Extract SQLModel entity with table=True: uses Pydantic-style typed fields + Relationship()
fn extract_sqlmodel_entity(
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

        // Skip non-field lines
        if !text.contains(':') || text.starts_with('#') {
            continue;
        }

        // name: type = Field(...) or name: type = Relationship(...)
        let parts: Vec<&str> = text.splitn(2, ':').collect();
        if parts.len() != 2 {
            continue;
        }

        let name = parts[0].trim().to_string();
        if name.starts_with('_') || name == "model_config" {
            continue;
        }

        let type_and_default = parts[1].trim();
        let field_type = type_and_default
            .split('=')
            .next()
            .unwrap_or("")
            .trim()
            .to_string();

        // Skip Relationship fields (they describe associations, not columns)
        if text.contains("Relationship(") {
            fields.push(EntityField {
                name,
                field_type: format!("relationship({})", field_type),
                constraints: vec!["relationship".to_string()],
            });
            continue;
        }

        let mut constraints = Vec::new();
        if text.contains("Field(") {
            if text.contains("primary_key=True") {
                constraints.push("primary_key".to_string());
            }
            if text.contains("unique=True") {
                constraints.push("unique".to_string());
            }
            if text.contains("index=True") {
                constraints.push("indexed".to_string());
            }
            if text.contains("foreign_key=") {
                constraints.push("relationship".to_string());
            }
            if text.contains("nullable=False") {
                constraints.push("required".to_string());
            }
            if let Some(v) = extract_kwarg_value(&text, "max_length") {
                constraints.push(format!("max_length={}", v));
            }
            if let Some(v) = extract_kwarg_value(&text, "min_length") {
                constraints.push(format!("min_length={}", v));
            }
        }

        // If no explicit nullable and not Optional/None default, it's required
        if !text.contains("| None")
            && !text.contains("Optional")
            && !text.contains("= None")
            && !constraints.contains(&"required".to_string())
        {
            constraints.push("required".to_string());
        }

        fields.push(EntityField {
            name,
            field_type,
            constraints,
        });
    }

    Some(Entity {
        name: class_name.to_string(),
        table: table_name,
        fields,
        bases: Vec::new(),
    })
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

        // field = Column(Type, ...) or field: Mapped[type] = mapped_column(...)
        if (text.contains("Column(")
            || text.contains("relationship(")
            || text.contains("mapped_column("))
            && let Some(field) = parse_sqlalchemy_field(&text)
        {
            fields.push(field);
        }
    }

    Some(Entity {
        name: class_name.to_string(),
        table: table_name,
        fields,
        bases: Vec::new(),
    })
}

fn parse_sqlalchemy_field(text: &str) -> Option<EntityField> {
    // Handle both old-style `name = Column(...)` and new-style `name: Mapped[type] = mapped_column(...)`
    let (name, rhs) = if text.contains("mapped_column(") {
        // name: Mapped[type] = mapped_column(Type, ...)
        let colon_parts: Vec<&str> = text.splitn(2, ':').collect();
        if colon_parts.len() != 2 {
            return None;
        }
        let field_name = colon_parts[0].trim().to_string();
        let after_colon = colon_parts[1].trim();
        // Find the `= mapped_column(` part
        if let Some(eq_pos) = after_colon.find("= mapped_column(") {
            (field_name, after_colon[eq_pos + 2..].trim().to_string())
        } else if let Some(eq_pos) = after_colon.find("=mapped_column(") {
            (field_name, after_colon[eq_pos + 1..].trim().to_string())
        } else {
            return None;
        }
    } else {
        let parts: Vec<&str> = text.splitn(2, '=').collect();
        if parts.len() != 2 {
            return None;
        }
        (parts[0].trim().to_string(), parts[1].trim().to_string())
    };

    if rhs.starts_with("Column(") || rhs.starts_with("mapped_column(") {
        let prefix_len = if rhs.starts_with("mapped_column(") {
            14
        } else {
            7
        };
        let inner = &rhs[prefix_len..rhs.rfind(')')?];
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

/// Filter out non-entity base class declarations (e.g. `class Base(DeclarativeBase)` itself)
fn is_base_class_definition(class_name: &str) -> bool {
    class_name == "Base" || class_name == "DeclarativeBase"
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
        bases: Vec::new(),
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
        bases: Vec::new(),
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

    #[test]
    fn test_sqlmodel_table_entity() {
        let source = r#"
from sqlmodel import Field, SQLModel, Relationship

class UserBase(SQLModel):
    email: str = Field(unique=True, max_length=255)
    is_active: bool = True

class User(UserBase, table=True):
    id: int = Field(primary_key=True)
    hashed_password: str
    items: list["Item"] = Relationship(back_populates="owner")
"#;

        let file = parse_file(source, "models.py");
        let capability = extract(&file).unwrap();

        // UserBase is Pydantic-like (no table=True), User is a table entity
        assert!(capability.entities.len() >= 2);

        let user = capability
            .entities
            .iter()
            .find(|e| e.name == "User")
            .unwrap();
        assert_eq!(user.table, "user");
        assert!(
            user.fields
                .iter()
                .any(|f| f.name == "id" && f.constraints.contains(&"primary_key".to_string()))
        );
        assert!(user.fields.iter().any(|f| f.name == "hashed_password"));
        assert!(
            user.fields
                .iter()
                .any(|f| f.constraints.contains(&"relationship".to_string()))
        );
    }

    #[test]
    fn test_sqlalchemy_mapped_column() {
        let source = r#"
from sqlalchemy.orm import Mapped, mapped_column
from sqlalchemy import Integer, String, Float

class Trade(Base):
    __tablename__ = "trades"

    id: Mapped[int] = mapped_column(Integer, primary_key=True)
    ticker: Mapped[str] = mapped_column(String(20))
    price: Mapped[float] = mapped_column(Float)
    status: Mapped[str] = mapped_column(String(20), unique=True)
"#;

        let file = parse_file(source, "models.py");
        let capability = extract(&file).unwrap();

        let trade = &capability.entities[0];
        assert_eq!(trade.name, "Trade");
        assert_eq!(trade.table, "trades");
        assert_eq!(trade.fields.len(), 4);
        assert!(
            trade.fields[0]
                .constraints
                .contains(&"primary_key".to_string())
        );
        assert_eq!(trade.fields[1].name, "ticker");
        assert_eq!(trade.fields[1].field_type, "String(20)");
        assert!(trade.fields[3].constraints.contains(&"unique".to_string()));
    }

    #[test]
    fn test_sqlmodel_inheritance_chain() {
        // Tests same-file inheritance resolution: SQLModel -> UserBase -> UserCreate
        let source = r#"
from sqlmodel import Field, SQLModel

class UserBase(SQLModel):
    email: str = Field(max_length=255)

class UserCreate(UserBase):
    password: str = Field(min_length=8)

class ItemBase(SQLModel):
    title: str

class ItemCreate(ItemBase):
    pass
"#;

        let file = parse_file(source, "models.py");
        let capability = extract(&file).unwrap();

        let names: Vec<&str> = capability
            .entities
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        assert!(
            names.contains(&"UserBase"),
            "Should find UserBase, got: {:?}",
            names
        );
        assert!(
            names.contains(&"UserCreate"),
            "Should find UserCreate (inherits UserBase), got: {:?}",
            names
        );
        assert!(
            names.contains(&"ItemBase"),
            "Should find ItemBase, got: {:?}",
            names
        );
        assert!(
            names.contains(&"ItemCreate"),
            "Should find ItemCreate (inherits ItemBase), got: {:?}",
            names
        );

        // UserCreate should have its own field (password)
        let user_create = capability
            .entities
            .iter()
            .find(|e| e.name == "UserCreate")
            .unwrap();
        assert_eq!(user_create.fields.len(), 1);
        assert_eq!(user_create.fields[0].name, "password");

        // ItemCreate has `pass` body → 0 fields (inherited fields not resolved)
        let item_create = capability
            .entities
            .iter()
            .find(|e| e.name == "ItemCreate")
            .unwrap();
        assert_eq!(item_create.fields.len(), 0);
    }

    #[test]
    fn test_base_class_not_entity() {
        let source = r#"
from sqlalchemy.orm import DeclarativeBase

class Base(DeclarativeBase):
    pass
"#;
        let file = parse_file(source, "database.py");
        assert!(extract(&file).is_none());
    }
}
