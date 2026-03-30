use super::common::*;
use crate::context::ParsedFile;
use pecto_core::model::*;

/// Extract entities from TypeScript files: TypeORM @Entity, Mongoose schemas.
pub fn extract(file: &ParsedFile) -> Option<Capability> {
    let full_text = &file.source;

    // Quick check
    if !full_text.contains("@Entity")
        && !full_text.contains("@Column")
        && !full_text.contains("new Schema(")
        && !full_text.contains("mongoose.Schema")
    {
        return None;
    }

    let mut entities = Vec::new();

    // TypeORM: @Entity() class with @Column decorators
    if full_text.contains("@Entity") {
        extract_typeorm_entities(full_text, &mut entities);
    }

    // Mongoose: new Schema({...})
    if full_text.contains("Schema(") {
        extract_mongoose_schemas(full_text, &mut entities);
    }

    if entities.is_empty() {
        return None;
    }

    let file_stem = file
        .path
        .rsplit('/')
        .next()
        .unwrap_or(&file.path)
        .split('.')
        .next()
        .unwrap_or("unknown");
    let capability_name = format!("{}-entity", to_kebab_case(file_stem));

    let mut capability = Capability::new(capability_name, file.path.clone());
    capability.entities = entities;
    Some(capability)
}

fn extract_typeorm_entities(source: &str, entities: &mut Vec<Entity>) {
    // Find @Entity() class blocks
    let mut remaining = source;
    while let Some(entity_pos) = remaining.find("@Entity(") {
        remaining = &remaining[entity_pos..];

        // Find class name and base class
        let (class_name, bases) = remaining
            .find("class ")
            .map(|pos| {
                let after = &remaining[pos + 6..];
                let line_end = after.find('{').unwrap_or(after.len());
                let class_line = &after[..line_end];
                let name = class_line
                    .split([' ', '{', '\n'])
                    .next()
                    .unwrap_or("Unknown")
                    .trim()
                    .to_string();
                let bases = if let Some(ext_pos) = class_line.find("extends ") {
                    let after_ext = &class_line[ext_pos + 8..];
                    let base = after_ext
                        .split([' ', '{', '\n', ','])
                        .next()
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    if base.is_empty() { Vec::new() } else { vec![base] }
                } else {
                    Vec::new()
                };
                (name, bases)
            })
            .unwrap_or_else(|| ("Unknown".to_string(), Vec::new()));

        // Find table name from @Entity('tablename') or default to class name
        let table_name = remaining
            .find("@Entity(")
            .and_then(|pos| {
                let after = &remaining[pos + 8..];
                let arg = after.split(')').next()?;
                if arg.contains('"') || arg.contains('\'') {
                    Some(clean_string_literal(arg.trim()))
                } else {
                    None
                }
            })
            .unwrap_or_else(|| class_name.to_lowercase());

        // Extract fields from @Column, @PrimaryGeneratedColumn, @ManyToOne, etc.
        let mut fields = Vec::new();

        // Find class body (between { and matching })
        if let Some(class_start) = remaining.find('{') {
            let class_body = &remaining[class_start..];
            let mut depth = 0;
            let mut end = class_body.len();
            for (i, c) in class_body.chars().enumerate() {
                match c {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            end = i;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let body = &class_body[1..end];
            extract_typeorm_fields(body, &mut fields);
        }

        entities.push(Entity {
            name: class_name,
            table: table_name,
            fields,
            bases,
        });

        // Move past this entity
        remaining = &remaining[1..];
        if let Some(next) = remaining.find("class ") {
            remaining = &remaining[next..];
        } else {
            break;
        }
    }
}

fn extract_typeorm_fields(body: &str, fields: &mut Vec<EntityField>) {
    let decorators = [
        "@PrimaryGeneratedColumn",
        "@PrimaryColumn",
        "@Column",
        "@ManyToOne",
        "@OneToMany",
        "@ManyToMany",
        "@OneToOne",
        "@JoinColumn",
    ];

    for line in body.lines() {
        let trimmed = line.trim();
        let has_decorator = decorators.iter().any(|d| trimmed.starts_with(d));
        if !has_decorator {
            continue;
        }

        let mut constraints = Vec::new();

        if trimmed.starts_with("@PrimaryGeneratedColumn") {
            constraints.push("@PrimaryGeneratedColumn".to_string());
        } else if trimmed.starts_with("@PrimaryColumn") {
            constraints.push("@PrimaryColumn".to_string());
        } else if trimmed.starts_with("@Column") {
            constraints.push("@Column".to_string());
            if trimmed.contains("nullable: false") || trimmed.contains("nullable:false") {
                constraints.push("required".to_string());
            }
            if trimmed.contains("unique: true") || trimmed.contains("unique:true") {
                constraints.push("unique".to_string());
            }
        } else if trimmed.starts_with("@ManyToOne") {
            constraints.push("@ManyToOne".to_string());
        } else if trimmed.starts_with("@OneToMany") {
            constraints.push("@OneToMany".to_string());
        } else if trimmed.starts_with("@ManyToMany") {
            constraints.push("@ManyToMany".to_string());
        } else if trimmed.starts_with("@OneToOne") {
            constraints.push("@OneToOne".to_string());
        }

        // Next non-decorator line should be the field: "name: string;"
        // We look at the line after the decorator in a simplified way
        // (the field name and type are often on the next line or same line after decorator)
    }

    // Simpler approach: find all "fieldName: Type" patterns after decorators
    let lines: Vec<&str> = body.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if decorators.iter().any(|d| trimmed.starts_with(d)) {
            let mut constraints = Vec::new();
            if trimmed.contains("PrimaryGeneratedColumn") || trimmed.contains("PrimaryColumn") {
                constraints.push("primary_key".to_string());
            }
            if trimmed.contains("ManyToOne") {
                constraints.push("@ManyToOne".to_string());
            }
            if trimmed.contains("OneToMany") {
                constraints.push("@OneToMany".to_string());
            }
            if trimmed.contains("ManyToMany") {
                constraints.push("@ManyToMany".to_string());
            }
            if trimmed.contains("nullable: false") {
                constraints.push("required".to_string());
            }
            if trimmed.contains("unique: true") {
                constraints.push("unique".to_string());
            }

            // Look at next line for field declaration
            if i + 1 < lines.len() {
                let next = lines[i + 1].trim();
                if next.contains(':') && !next.starts_with('@') && !next.starts_with("//") {
                    let parts: Vec<&str> = next.splitn(2, ':').collect();
                    if parts.len() == 2 {
                        let name = parts[0]
                            .trim()
                            .trim_start_matches("readonly ")
                            .trim()
                            .to_string();
                        let field_type = parts[1].trim().trim_end_matches(';').trim().to_string();
                        if !name.is_empty() && !name.starts_with("//") {
                            fields.push(EntityField {
                                name,
                                field_type,
                                constraints,
                            });
                        }
                    }
                }
            }
        }
        i += 1;
    }
}

fn extract_mongoose_schemas(source: &str, entities: &mut Vec<Entity>) {
    // Find: const XxxSchema = new Schema({ ... })
    // or: const XxxSchema = new mongoose.Schema({ ... })
    let mut remaining = source;
    while let Some(pos) = remaining.find("Schema(") {
        // Look back for variable name
        let before = &remaining[..pos];
        let schema_name = before
            .rsplit([' ', '\t', '='])
            .find(|s| !s.is_empty() && *s != "new" && *s != "mongoose.")
            .map(|s| {
                s.trim()
                    .replace("Schema", "")
                    .replace("const ", "")
                    .replace("let ", "")
            })
            .unwrap_or_else(|| "Unknown".to_string());

        let name = if schema_name.is_empty() || schema_name == "new" {
            "Unknown".to_string()
        } else {
            schema_name
        };

        entities.push(Entity {
            name: name.clone(),
            table: name.to_lowercase(),
            fields: Vec::new(), // Mongoose schema fields are complex to parse from text
            bases: Vec::new(),
        });

        remaining = &remaining[pos + 7..];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ParsedFile;

    fn parse_file(source: &str, path: &str) -> ParsedFile {
        ParsedFile::parse(source.to_string(), path.to_string()).unwrap()
    }

    #[test]
    fn test_typeorm_entity() {
        let source = r#"
import { Entity, PrimaryGeneratedColumn, Column, ManyToOne } from 'typeorm';

@Entity('users')
export class User {
    @PrimaryGeneratedColumn()
    id: number;

    @Column({ nullable: false, unique: true })
    email: string;

    @Column()
    name: string;

    @ManyToOne(() => Organization)
    organization: Organization;
}
"#;

        let file = parse_file(source, "entities/user.entity.ts");
        let capability = extract(&file).unwrap();

        let entity = &capability.entities[0];
        assert_eq!(entity.name, "User");
        assert_eq!(entity.table, "users");
        assert!(
            entity.fields.len() >= 3,
            "Should find fields, found {}",
            entity.fields.len()
        );
    }

    #[test]
    fn test_no_entity() {
        let source = r#"
export class UserService {
    findAll() { return []; }
}
"#;
        let file = parse_file(source, "user.service.ts");
        assert!(extract(&file).is_none());
    }
}
