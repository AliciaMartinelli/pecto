use super::common::*;
use crate::context::{AnalysisContext, ParsedFile};
use pecto_core::model::*;
use std::collections::BTreeMap;
use tree_sitter::Node;

/// ASP.NET Core StatusCodes mappings.
const STATUS_MAPPINGS: &[(&str, u16, &str)] = &[
    ("Status200OK", 200, "success"),
    ("Status201Created", 201, "created"),
    ("Status202Accepted", 202, "accepted"),
    ("Status204NoContent", 204, "no-content"),
    ("Status400BadRequest", 400, "bad-request"),
    ("Status401Unauthorized", 401, "unauthorized"),
    ("Status403Forbidden", 403, "forbidden"),
    ("Status404NotFound", 404, "not-found"),
    ("Status409Conflict", 409, "conflict"),
    ("Status422UnprocessableEntity", 422, "unprocessable-entity"),
    ("Status500InternalServerError", 500, "internal-error"),
];

/// Extract a Capability from a parsed file if it contains an ASP.NET Core controller.
pub fn extract(file: &ParsedFile, ctx: &AnalysisContext) -> Option<Capability> {
    let root = file.tree.root_node();
    let source = file.source.as_bytes();

    let mut result = None;
    for_each_class(&root, source, &mut |node, src| {
        if result.is_some() {
            return;
        }

        let attrs = collect_attributes(node, src);
        let is_controller = attrs.iter().any(|a| a.name == "ApiController")
            || (get_class_name(node, src).ends_with("Controller")
                && attrs.iter().any(|a| a.name == "Route"));

        if !is_controller {
            return;
        }

        let class_name = get_class_name(node, src);
        let base_path = extract_route_template(&attrs, &class_name);
        let capability_name = to_kebab_case(&class_name.replace("Controller", ""));

        let mut capability = Capability::new(capability_name, file.path.clone());

        if let Some(body) = node.child_by_field_name("body") {
            extract_endpoints_from_class(&body, src, &base_path, &attrs, &mut capability, ctx);
        }

        result = Some(capability);
    });

    result
}

/// Extract route template from [Route("api/[controller]")] and substitute [controller].
fn extract_route_template(attrs: &[AttributeInfo], class_name: &str) -> String {
    for attr in attrs {
        if attr.name == "Route"
            && let Some(val) = &attr.value
        {
            let controller_name = class_name
                .strip_suffix("Controller")
                .unwrap_or(class_name)
                .to_lowercase();
            return val.replace("[controller]", &controller_name);
        }
    }
    String::new()
}

fn extract_endpoints_from_class(
    body: &Node,
    source: &[u8],
    base_path: &str,
    class_attrs: &[AttributeInfo],
    capability: &mut Capability,
    ctx: &AnalysisContext,
) {
    // Class-level security
    let class_auth = class_attrs.iter().any(|a| a.name == "Authorize");
    let class_roles = extract_roles(class_attrs);

    for i in 0..body.named_child_count() {
        let member = body.named_child(i).unwrap();
        if member.kind() == "method_declaration" {
            let attrs = collect_attributes(&member, source);
            if let Some(endpoint) = extract_endpoint(
                &member,
                source,
                &attrs,
                base_path,
                class_auth,
                &class_roles,
                ctx,
            ) {
                capability.endpoints.push(endpoint);
            }
        }
    }
}

fn extract_endpoint(
    method_node: &Node,
    source: &[u8],
    attrs: &[AttributeInfo],
    base_path: &str,
    class_auth: bool,
    class_roles: &[String],
    ctx: &AnalysisContext,
) -> Option<Endpoint> {
    let (http_method, method_path) = extract_http_method_and_path(attrs)?;

    let full_path = if base_path.is_empty() {
        method_path
    } else if method_path.is_empty() {
        base_path.to_string()
    } else {
        format!(
            "{}/{}",
            base_path.trim_end_matches('/'),
            method_path.trim_start_matches('/')
        )
    };

    let input = extract_method_input(method_node, source, ctx);
    let validation = extract_validation_rules(method_node, source, ctx);
    let return_type = extract_return_type(method_node, source);
    let security = extract_security(attrs, class_auth, class_roles);

    // Default success behavior
    let success_status = extract_produces_status(attrs, true).unwrap_or(200);
    let mut behaviors = vec![Behavior {
        name: "success".to_string(),
        condition: None,
        returns: ResponseSpec {
            status: success_status,
            body: return_type.map(|t| TypeRef {
                name: t,
                fields: BTreeMap::new(),
            }),
        },
        side_effects: Vec::new(),
    }];

    // Error behaviors from [ProducesResponseType]
    extract_error_behaviors(attrs, &mut behaviors);

    Some(Endpoint {
        method: http_method,
        path: full_path,
        input,
        validation,
        behaviors,
        security,
    })
}

fn extract_http_method_and_path(attrs: &[AttributeInfo]) -> Option<(HttpMethod, String)> {
    for attr in attrs {
        let (method, path) = match attr.name.as_str() {
            "HttpGet" => (HttpMethod::Get, attr.value.clone().unwrap_or_default()),
            "HttpPost" => (HttpMethod::Post, attr.value.clone().unwrap_or_default()),
            "HttpPut" => (HttpMethod::Put, attr.value.clone().unwrap_or_default()),
            "HttpDelete" => (HttpMethod::Delete, attr.value.clone().unwrap_or_default()),
            "HttpPatch" => (HttpMethod::Patch, attr.value.clone().unwrap_or_default()),
            _ => continue,
        };
        // Convert {id} to keep consistent with Java's {id} format
        return Some((method, path));
    }
    None
}

fn extract_method_input(
    method_node: &Node,
    source: &[u8],
    ctx: &AnalysisContext,
) -> Option<EndpointInput> {
    let params = method_node.child_by_field_name("parameters")?;
    let mut body = None;
    let mut path_params = Vec::new();
    let mut query_params = Vec::new();

    for i in 0..params.named_child_count() {
        let param = params.named_child(i).unwrap();
        if param.kind() != "parameter" {
            continue;
        }

        let param_attrs = collect_attributes(&param, source);
        let param_type = param
            .child_by_field_name("type")
            .map(|t| node_text(&t, source))
            .unwrap_or_default();
        let param_name = param
            .child_by_field_name("name")
            .map(|n| node_text(&n, source))
            .unwrap_or_default();

        let binding = detect_binding(&param_attrs);

        match binding {
            ParamBinding::Body => {
                let type_name = clean_generic_type(&param_type);
                let fields = resolve_type_fields(&type_name, ctx);
                body = Some(TypeRef {
                    name: type_name,
                    fields,
                });
            }
            ParamBinding::Route => {
                path_params.push(Param {
                    name: param_name,
                    param_type,
                    required: true,
                });
            }
            ParamBinding::Query => {
                query_params.push(Param {
                    name: param_name,
                    param_type,
                    required: false,
                });
            }
            ParamBinding::None => {
                // No explicit binding — check if name matches a route param
                // or treat as query param for simple types
            }
        }
    }

    if body.is_none() && path_params.is_empty() && query_params.is_empty() {
        return None;
    }

    Some(EndpointInput {
        body,
        path_params,
        query_params,
    })
}

enum ParamBinding {
    Body,
    Route,
    Query,
    None,
}

fn detect_binding(attrs: &[AttributeInfo]) -> ParamBinding {
    for attr in attrs {
        match attr.name.as_str() {
            "FromBody" => return ParamBinding::Body,
            "FromRoute" => return ParamBinding::Route,
            "FromQuery" => return ParamBinding::Query,
            _ => {}
        }
    }
    ParamBinding::None
}

fn extract_return_type(method_node: &Node, source: &[u8]) -> Option<String> {
    // C# tree-sitter uses "returns" field for return type, or "type"
    let raw = if let Some(type_node) = method_node
        .child_by_field_name("returns")
        .or_else(|| method_node.child_by_field_name("type"))
    {
        node_text(&type_node, source)
    } else {
        return None;
    };

    let cleaned = clean_generic_type(&raw);

    if cleaned == "void" || cleaned == "IActionResult" || cleaned == "Task" {
        return None;
    }

    Some(cleaned)
}

fn extract_security(
    attrs: &[AttributeInfo],
    class_auth: bool,
    class_roles: &[String],
) -> Option<SecurityConfig> {
    // [AllowAnonymous] overrides class-level auth
    if attrs.iter().any(|a| a.name == "AllowAnonymous") {
        return None;
    }

    let method_auth = attrs.iter().any(|a| a.name == "Authorize");
    let method_roles = extract_roles(attrs);

    let has_auth = method_auth || class_auth;
    let mut roles = class_roles.to_vec();
    roles.extend(method_roles);

    if !has_auth && roles.is_empty() {
        return None;
    }

    Some(SecurityConfig {
        authentication: if has_auth {
            Some("required".to_string())
        } else {
            None
        },
        roles,
        rate_limit: None,
        cors: None,
    })
}

fn extract_roles(attrs: &[AttributeInfo]) -> Vec<String> {
    let mut roles = Vec::new();
    for attr in attrs {
        if attr.name == "Authorize"
            && let Some(r) = attr.arguments.get("Roles")
        {
            roles.push(r.clone());
        }
    }
    roles
}

/// Extract success or error status from [ProducesResponseType] attributes.
fn extract_produces_status(attrs: &[AttributeInfo], success_only: bool) -> Option<u16> {
    for attr in attrs {
        if attr.name == "ProducesResponseType"
            && let Some(status) = resolve_status_code(attr)
            && success_only
            && status < 400
        {
            return Some(status);
        }
    }
    None
}

fn extract_error_behaviors(attrs: &[AttributeInfo], behaviors: &mut Vec<Behavior>) {
    for attr in attrs {
        if attr.name == "ProducesResponseType"
            && let Some(status) = resolve_status_code(attr)
            && status >= 400
        {
            let name = status_to_behavior_name(status);
            if !behaviors.iter().any(|b| b.name == name) {
                behaviors.push(Behavior {
                    name,
                    condition: None,
                    returns: ResponseSpec { status, body: None },
                    side_effects: Vec::new(),
                });
            }
        }
    }
}

fn resolve_status_code(attr: &AttributeInfo) -> Option<u16> {
    // Check value and arguments for status code references
    let text = attr
        .value
        .as_ref()
        .or_else(|| attr.arguments.values().next())?;

    // Try StatusCodes.StatusXXX pattern
    for &(name, code, _) in STATUS_MAPPINGS {
        if text.contains(name) {
            return Some(code);
        }
    }

    // Try direct number
    text.parse::<u16>().ok()
}

fn status_to_behavior_name(status: u16) -> String {
    for &(_, code, name) in STATUS_MAPPINGS {
        if code == status {
            return name.to_string();
        }
    }
    format!("error-{}", status)
}

fn extract_validation_rules(
    method_node: &Node,
    source: &[u8],
    ctx: &AnalysisContext,
) -> Vec<ValidationRule> {
    let mut rules = Vec::new();
    let Some(params) = method_node.child_by_field_name("parameters") else {
        return rules;
    };

    for i in 0..params.named_child_count() {
        let param = params.named_child(i).unwrap();
        if param.kind() != "parameter" {
            continue;
        }

        let param_attrs = collect_attributes(&param, source);
        if !param_attrs.iter().any(|a| a.name == "FromBody") {
            continue;
        }

        let param_type = param
            .child_by_field_name("type")
            .map(|t| node_text(&t, source))
            .unwrap_or_default();
        let type_name = clean_generic_type(&param_type);

        let field_rules = resolve_validation_rules(&type_name, ctx);
        rules.extend(field_rules);
    }

    rules
}

fn resolve_type_fields(type_name: &str, ctx: &AnalysisContext) -> BTreeMap<String, String> {
    let mut fields = BTreeMap::new();
    let Some(file) = ctx.find_class_by_name(type_name) else {
        return fields;
    };

    let root = file.tree.root_node();
    let source = file.source.as_bytes();

    for_each_class(&root, source, &mut |node, src| {
        if get_class_name(node, src) != type_name {
            return;
        }
        if let Some(body) = node.child_by_field_name("body") {
            for j in 0..body.named_child_count() {
                let member = body.named_child(j).unwrap();
                if member.kind() == "property_declaration" {
                    let prop_type = member
                        .child_by_field_name("type")
                        .map(|t| node_text(&t, src))
                        .unwrap_or_default();
                    let prop_name = member
                        .child_by_field_name("name")
                        .map(|n| node_text(&n, src))
                        .unwrap_or_default();
                    if !prop_name.is_empty() {
                        fields.insert(prop_name, prop_type);
                    }
                }
            }
        }
    });

    fields
}

fn resolve_validation_rules(type_name: &str, ctx: &AnalysisContext) -> Vec<ValidationRule> {
    let mut rules = Vec::new();
    let Some(file) = ctx.find_class_by_name(type_name) else {
        return rules;
    };

    let root = file.tree.root_node();
    let source = file.source.as_bytes();

    for_each_class(&root, source, &mut |node, src| {
        if get_class_name(node, src) != type_name {
            return;
        }
        if let Some(body) = node.child_by_field_name("body") {
            for j in 0..body.named_child_count() {
                let member = body.named_child(j).unwrap();
                if member.kind() != "property_declaration" {
                    continue;
                }
                let attrs = collect_attributes(&member, src);
                let constraints = extract_validation_constraints(&attrs);
                if constraints.is_empty() {
                    continue;
                }
                let prop_name = member
                    .child_by_field_name("name")
                    .map(|n| node_text(&n, src))
                    .unwrap_or_default();
                if !prop_name.is_empty() {
                    rules.push(ValidationRule {
                        field: prop_name,
                        constraints,
                    });
                }
            }
        }
    });

    rules
}

fn extract_validation_constraints(attrs: &[AttributeInfo]) -> Vec<String> {
    let mut constraints = Vec::new();
    for attr in attrs {
        match attr.name.as_str() {
            "Required" => constraints.push("[Required]".to_string()),
            "EmailAddress" => constraints.push("[EmailAddress]".to_string()),
            "Phone" => constraints.push("[Phone]".to_string()),
            "Url" => constraints.push("[Url]".to_string()),
            "CreditCard" => constraints.push("[CreditCard]".to_string()),
            "StringLength" => {
                let val = attr.value.as_deref().unwrap_or("?");
                if let Some(min) = attr.arguments.get("MinimumLength") {
                    constraints.push(format!("[StringLength({}, MinimumLength={})]", val, min));
                } else {
                    constraints.push(format!("[StringLength({})]", val));
                }
            }
            "MaxLength" => {
                let val = attr.value.as_deref().unwrap_or("?");
                constraints.push(format!("[MaxLength({})]", val));
            }
            "MinLength" => {
                let val = attr.value.as_deref().unwrap_or("?");
                constraints.push(format!("[MinLength({})]", val));
            }
            "Range" => {
                let val = attr.value.as_deref().unwrap_or("?");
                constraints.push(format!("[Range({})]", val));
            }
            "RegularExpression" => {
                let val = attr.value.as_deref().unwrap_or("?");
                constraints.push(format!("[RegularExpression({})]", val));
            }
            "Compare" => {
                let val = attr.value.as_deref().unwrap_or("?");
                constraints.push(format!("[Compare({})]", val));
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
    fn test_basic_controller() {
        let source = r#"
using Microsoft.AspNetCore.Mvc;

namespace MyApp.Controllers;

[ApiController]
[Route("api/[controller]")]
public class UsersController : ControllerBase
{
    [HttpGet]
    public ActionResult<List<User>> GetAll()
    {
        return Ok(users);
    }

    [HttpGet("{id}")]
    public ActionResult<User> GetById([FromRoute] int id)
    {
        return Ok(user);
    }

    [HttpPost]
    public ActionResult<User> Create([FromBody] CreateUserRequest request)
    {
        return CreatedAtAction(nameof(GetById), new { id = user.Id }, user);
    }

    [HttpDelete("{id}")]
    public IActionResult Delete([FromRoute] int id)
    {
        return NoContent();
    }
}
"#;

        let file = parse_file(source, "Controllers/UsersController.cs");
        let capability = extract(&file, &empty_ctx()).unwrap();

        assert_eq!(capability.name, "users");
        assert_eq!(capability.endpoints.len(), 4);

        // GET api/users
        let get_all = &capability.endpoints[0];
        assert!(matches!(get_all.method, HttpMethod::Get));
        assert_eq!(get_all.path, "api/users");

        // GET api/users/{id}
        let get_by_id = &capability.endpoints[1];
        assert!(matches!(get_by_id.method, HttpMethod::Get));
        assert_eq!(get_by_id.path, "api/users/{id}");
        assert_eq!(get_by_id.input.as_ref().unwrap().path_params.len(), 1);

        // POST api/users
        let create = &capability.endpoints[2];
        assert!(matches!(create.method, HttpMethod::Post));
        assert!(create.input.as_ref().unwrap().body.is_some());

        // DELETE api/users/{id}
        let delete = &capability.endpoints[3];
        assert!(matches!(delete.method, HttpMethod::Delete));
    }

    #[test]
    fn test_async_return_types() {
        let source = r#"
[ApiController]
[Route("api/orders")]
public class OrdersController : ControllerBase
{
    [HttpGet("{id}")]
    public async Task<ActionResult<Order>> GetById([FromRoute] int id)
    {
        return Ok(order);
    }

    [HttpPost]
    public async Task<IActionResult> Create([FromBody] CreateOrderRequest request)
    {
        return Ok();
    }
}
"#;

        let file = parse_file(source, "Controllers/OrdersController.cs");
        let capability = extract(&file, &empty_ctx()).unwrap();

        // Task<ActionResult<Order>> → Order
        let get = &capability.endpoints[0];
        let body = get.behaviors[0].returns.body.as_ref();
        assert!(
            body.is_some(),
            "Should extract return type from Task<ActionResult<Order>>, got None. Behaviors: {:?}",
            get.behaviors
        );
        assert_eq!(body.unwrap().name, "Order");

        // Task<IActionResult> → no body
        let post = &capability.endpoints[1];
        assert!(post.behaviors[0].returns.body.is_none());
    }

    #[test]
    fn test_authorize_attribute() {
        let source = r#"
[ApiController]
[Route("api/admin")]
[Authorize]
public class AdminController : ControllerBase
{
    [HttpGet]
    [Authorize(Roles = "SuperAdmin")]
    public IActionResult GetSecrets()
    {
        return Ok();
    }

    [HttpGet("public")]
    [AllowAnonymous]
    public IActionResult GetPublic()
    {
        return Ok();
    }
}
"#;

        let file = parse_file(source, "Controllers/AdminController.cs");
        let capability = extract(&file, &empty_ctx()).unwrap();

        // GetSecrets: auth required + role
        let secrets = &capability.endpoints[0];
        let sec = secrets.security.as_ref().unwrap();
        assert_eq!(sec.authentication.as_deref(), Some("required"));
        assert!(sec.roles.iter().any(|r| r.contains("SuperAdmin")));

        // GetPublic: AllowAnonymous overrides class-level Authorize
        let public_ep = &capability.endpoints[1];
        assert!(public_ep.security.is_none());
    }

    #[test]
    fn test_produces_response_type() {
        let source = r#"
[ApiController]
[Route("api/items")]
public class ItemsController : ControllerBase
{
    [HttpGet("{id}")]
    [ProducesResponseType(StatusCodes.Status200OK)]
    [ProducesResponseType(StatusCodes.Status404NotFound)]
    public ActionResult<Item> GetById([FromRoute] int id)
    {
        return Ok(item);
    }
}
"#;

        let file = parse_file(source, "Controllers/ItemsController.cs");
        let capability = extract(&file, &empty_ctx()).unwrap();

        let endpoint = &capability.endpoints[0];
        assert!(endpoint.behaviors.iter().any(|b| b.name == "success"));
        assert!(
            endpoint
                .behaviors
                .iter()
                .any(|b| b.name == "not-found" && b.returns.status == 404)
        );
    }

    #[test]
    fn test_non_controller_class() {
        let source = r#"
namespace MyApp.Services;

public class UserService
{
    public User FindById(int id) { return null; }
}
"#;

        let file = parse_file(source, "Services/UserService.cs");
        assert!(extract(&file, &empty_ctx()).is_none());
    }
}
