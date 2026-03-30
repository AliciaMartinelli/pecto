use super::common::*;
use crate::context::ParsedFile;
use pecto_core::model::*;

/// Extract services from Python classes ending in Service, or decorated with @inject,
/// or module-level functions in files with service/crud/repository in the path.
pub fn extract(file: &ParsedFile) -> Option<Capability> {
    let root = file.tree.root_node();
    let source = file.source.as_bytes();

    // First try class-based service extraction
    if let Some(cap) = extract_class_service(root, source, file) {
        return Some(cap);
    }

    // Fallback: module-level functions in service/crud/repository files
    extract_module_functions(root, source, file)
}

fn extract_class_service(
    root: tree_sitter::Node,
    source: &[u8],
    file: &ParsedFile,
) -> Option<Capability> {
    let mut operations = Vec::new();
    let mut class_name = String::new();

    for i in 0..root.named_child_count() {
        let node = root.named_child(i).unwrap();

        let (decorators, class_node) = if node.kind() == "class_definition" {
            (Vec::new(), node)
        } else if node.kind() == "decorated_definition" {
            let decs = collect_decorators(&node, source);
            match get_inner_definition(&node) {
                Some(n) if n.kind() == "class_definition" => (decs, n),
                _ => continue,
            }
        } else {
            continue;
        };

        let name = get_def_name(&class_node, source);
        let is_service = name.ends_with("Service")
            || name.ends_with("Repository")
            || name.ends_with("UseCase")
            || decorators
                .iter()
                .any(|d| d.name == "inject" || d.name == "injectable" || d.name == "service");

        if !is_service {
            continue;
        }

        class_name = name.clone();

        // Extract public methods as operations
        if let Some(body) = class_node.child_by_field_name("body") {
            for j in 0..body.named_child_count() {
                let member = body.named_child(j).unwrap();

                let func = if member.kind() == "function_definition" {
                    member
                } else if member.kind() == "decorated_definition" {
                    match get_inner_definition(&member) {
                        Some(n) if n.kind() == "function_definition" => n,
                        _ => continue,
                    }
                } else {
                    continue;
                };

                let method_name = get_def_name(&func, source);

                // Skip private/dunder methods
                if method_name.starts_with('_') {
                    continue;
                }

                operations.push(make_operation(&func, source, &name, &method_name));
            }
        }

        break; // One service per file
    }

    if operations.is_empty() {
        return None;
    }

    let capability_name = to_kebab_case(
        &class_name
            .replace("Service", "")
            .replace("Repository", "")
            .replace("UseCase", ""),
    );
    let mut capability = Capability::new(format!("{}-service", capability_name), file.path.clone());
    capability.operations = operations;
    Some(capability)
}

/// Extract module-level functions as service operations from files whose name
/// suggests they contain service logic (crud.py, services.py, repository.py, etc.)
fn extract_module_functions(
    root: tree_sitter::Node,
    source: &[u8],
    file: &ParsedFile,
) -> Option<Capability> {
    let file_stem = file
        .path
        .rsplit('/')
        .next()
        .unwrap_or(&file.path)
        .trim_end_matches(".py")
        .to_lowercase();

    let is_service_file = file_stem == "crud"
        || file_stem == "services"
        || file_stem == "service"
        || file_stem == "repository"
        || file_stem == "queries"
        || file_stem == "actions"
        || file_stem.ends_with("_service")
        || file_stem.ends_with("_crud")
        || file_stem.ends_with("_repository");

    if !is_service_file {
        return None;
    }

    let mut operations = Vec::new();
    let module_name = to_kebab_case(&file_stem);

    for i in 0..root.named_child_count() {
        let node = root.named_child(i).unwrap();

        let func = if node.kind() == "function_definition" {
            node
        } else if node.kind() == "decorated_definition" {
            match get_inner_definition(&node) {
                Some(n) if n.kind() == "function_definition" => n,
                _ => continue,
            }
        } else {
            continue;
        };

        let func_name = get_def_name(&func, source);

        // Skip private functions
        if func_name.starts_with('_') {
            continue;
        }

        operations.push(make_operation(&func, source, &file_stem, &func_name));
    }

    if operations.is_empty() {
        return None;
    }

    let mut capability = Capability::new(format!("{}-service", module_name), file.path.clone());
    capability.operations = operations;
    Some(capability)
}

fn make_operation(
    func: &tree_sitter::Node,
    source: &[u8],
    owner_name: &str,
    method_name: &str,
) -> Operation {
    let input = func
        .child_by_field_name("parameters")
        .and_then(|p| extract_first_non_self_param(&p, source));

    let return_type = func
        .child_by_field_name("return_type")
        .map(|t| node_text(&t, source))
        .filter(|t| t != "None" && !t.is_empty());

    Operation {
        name: method_name.to_string(),
        source_method: format!("{}#{}", owner_name, method_name),
        input: input.map(|t| TypeRef {
            name: t,
            fields: std::collections::BTreeMap::new(),
        }),
        behaviors: vec![Behavior {
            name: "success".to_string(),
            condition: None,
            returns: ResponseSpec {
                status: 200,
                body: return_type.map(|t| TypeRef {
                    name: t,
                    fields: std::collections::BTreeMap::new(),
                }),
            },
            side_effects: Vec::new(),
        }],
        transaction: None,
    }
}

fn extract_first_non_self_param(params: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    for i in 0..params.named_child_count() {
        let param = params.named_child(i).unwrap();
        let name = match param.kind() {
            "typed_parameter" | "typed_default_parameter" => param
                .child_by_field_name("name")
                .map(|n| node_text(&n, source))
                .unwrap_or_default(),
            "identifier" => node_text(&param, source),
            _ => continue,
        };

        if name == "self" || name == "cls" {
            continue;
        }

        // Return the type if available
        if let Some(type_node) = param.child_by_field_name("type") {
            return Some(node_text(&type_node, source));
        }
        return Some(name);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ParsedFile;

    fn parse_file(source: &str, path: &str) -> ParsedFile {
        ParsedFile::parse(source.to_string(), path.to_string()).unwrap()
    }

    #[test]
    fn test_service_extraction() {
        let source = r#"
class UserService:
    def __init__(self, db: Database):
        self.db = db

    def find_by_id(self, user_id: int) -> User:
        return self.db.get(user_id)

    def create(self, data: UserCreate) -> User:
        return self.db.create(data)

    def _private_helper(self):
        pass
"#;

        let file = parse_file(source, "services/user_service.py");
        let capability = extract(&file).unwrap();

        assert_eq!(capability.name, "user-service");
        // find_by_id + create (private skipped)
        assert_eq!(capability.operations.len(), 2);
        assert_eq!(capability.operations[0].name, "find_by_id");
        assert_eq!(capability.operations[1].name, "create");
    }

    #[test]
    fn test_non_service() {
        let source = r#"
class Helper:
    def do_thing(self):
        pass
"#;
        let file = parse_file(source, "utils.py");
        assert!(extract(&file).is_none());
    }

    #[test]
    fn test_module_level_crud() {
        let source = r#"
from sqlmodel import Session
from app.models import User, UserCreate

def create_user(session: Session, user_create: UserCreate) -> User:
    db_obj = User.model_validate(user_create)
    session.add(db_obj)
    session.commit()
    return db_obj

def get_user_by_email(session: Session, email: str) -> User:
    return session.exec(select(User).where(User.email == email)).first()

def _private_helper():
    pass
"#;

        let file = parse_file(source, "app/crud.py");
        let capability = extract(&file).unwrap();

        assert_eq!(capability.name, "crud-service");
        assert_eq!(capability.operations.len(), 2);
        assert_eq!(capability.operations[0].name, "create_user");
        assert_eq!(capability.operations[1].name, "get_user_by_email");
    }

    #[test]
    fn test_module_level_non_crud_file() {
        let source = r#"
def helper_func():
    pass
"#;
        let file = parse_file(source, "utils.py");
        assert!(extract(&file).is_none());
    }
}
