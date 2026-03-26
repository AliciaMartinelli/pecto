use super::common::*;
use crate::context::ParsedFile;
use pecto_core::model::*;

/// Extract NestJS @Injectable services or classes ending in Service.
pub fn extract(file: &ParsedFile) -> Option<Capability> {
    let full_text = &file.source;

    if !full_text.contains("@Injectable")
        && !full_text.contains("Service")
        && !full_text.contains("Repository")
    {
        return None;
    }

    // Find service classes
    let mut operations = Vec::new();
    let mut class_name = String::new();

    // Text-based: find @Injectable() or class XxxService
    for line in full_text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("export class ") || trimmed.starts_with("class ") {
            let name = trimmed
                .replace("export ", "")
                .replace("class ", "")
                .split([' ', '{', '<'])
                .next()
                .unwrap_or("")
                .trim()
                .to_string();

            let is_service = name.ends_with("Service")
                || name.ends_with("Repository")
                || name.ends_with("UseCase")
                || full_text.contains("@Injectable");

            if is_service && !name.contains("Controller") {
                class_name = name;
                break;
            }
        }
    }

    if class_name.is_empty() {
        return None;
    }

    // Extract public methods (simplified: non-private, non-constructor methods)
    let mut in_class = false;
    let mut depth = 0;
    for line in full_text.lines() {
        let trimmed = line.trim();

        if trimmed.contains(&format!("class {}", class_name)) {
            in_class = true;
        }

        if in_class {
            depth += trimmed.matches('{').count();
            depth -= trimmed.matches('}').count();

            if depth == 0 && in_class {
                break;
            }

            // Match method declarations (at class body level, depth=2)
            if depth == 2
                && !trimmed.starts_with("//")
                && !trimmed.starts_with("private ")
                && !trimmed.starts_with("constructor")
                && !trimmed.starts_with("@")
                && !trimmed.starts_with("readonly ")
                && !trimmed.is_empty()
                && (trimmed.contains("(") && trimmed.contains(")") || trimmed.ends_with("{"))
            {
                let method_name = trimmed
                    .replace("async ", "")
                    .replace("public ", "")
                    .split('(')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string();

                if !method_name.is_empty()
                    && !method_name.starts_with("get ")
                    && !method_name.starts_with("set ")
                    && method_name != "constructor"
                {
                    operations.push(Operation {
                        name: method_name.clone(),
                        source_method: format!("{}#{}", class_name, method_name),
                        input: None,
                        behaviors: vec![Behavior {
                            name: "success".to_string(),
                            condition: None,
                            returns: ResponseSpec {
                                status: 200,
                                body: None,
                            },
                            side_effects: Vec::new(),
                        }],
                        transaction: None,
                    });
                }
            }
        }
    }

    if operations.is_empty() {
        return None;
    }

    let capability_name =
        to_kebab_case(&class_name.replace("Service", "").replace("Repository", ""));
    let mut capability = Capability::new(format!("{}-service", capability_name), file.path.clone());
    capability.operations = operations;
    Some(capability)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ParsedFile;

    fn parse_file(source: &str, path: &str) -> ParsedFile {
        ParsedFile::parse(source.to_string(), path.to_string()).unwrap()
    }

    #[test]
    fn test_nestjs_service() {
        let source = r#"
import { Injectable } from '@nestjs/common';

@Injectable()
export class UsersService {
    constructor(private readonly usersRepository: UsersRepository) {}

    async findAll(): Promise<User[]> {
        return this.usersRepository.find();
    }

    async findOne(id: number): Promise<User> {
        return this.usersRepository.findOne(id);
    }

    async create(dto: CreateUserDto): Promise<User> {
        return this.usersRepository.save(dto);
    }

    private validate(dto: CreateUserDto): boolean {
        return true;
    }
}
"#;

        let file = parse_file(source, "users.service.ts");
        let capability = extract(&file).unwrap();

        assert_eq!(capability.name, "users-service");
        assert!(
            capability.operations.len() >= 3,
            "Should find 3+ operations, found {}",
            capability.operations.len()
        );
    }

    #[test]
    fn test_non_service() {
        let source = r#"
export class UserDto {
    name: string;
    email: string;
}
"#;
        let file = parse_file(source, "user.dto.ts");
        assert!(extract(&file).is_none());
    }
}
