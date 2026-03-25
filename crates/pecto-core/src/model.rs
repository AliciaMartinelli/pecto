use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A complete behavior spec for a project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSpec {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analyzed: Option<String>,
    pub files_analyzed: usize,
    pub capabilities: Vec<Capability>,
}

/// A capability groups related behaviors (e.g., "user-authentication", "order-management").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capability {
    pub name: String,
    pub source: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub endpoints: Vec<Endpoint>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub operations: Vec<Operation>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub entities: Vec<Entity>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub scheduled_tasks: Vec<ScheduledTask>,
}

/// An HTTP endpoint extracted from a controller.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Endpoint {
    pub method: HttpMethod,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<EndpointInput>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub validation: Vec<ValidationRule>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub behaviors: Vec<Behavior>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security: Option<SecurityConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
    Patch,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<TypeRef>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub path_params: Vec<Param>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub query_params: Vec<Param>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Param {
    pub name: String,
    #[serde(rename = "type")]
    pub param_type: String,
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub required: bool,
}

/// A reference to a type with its fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeRef {
    pub name: String,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub fields: BTreeMap<String, String>,
}

/// A validation rule on an input field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationRule {
    pub field: String,
    pub constraints: Vec<String>,
}

/// A behavior describes what happens under specific conditions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Behavior {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    pub returns: ResponseSpec,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub side_effects: Vec<SideEffect>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseSpec {
    pub status: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<TypeRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum SideEffect {
    #[serde(rename = "db_insert")]
    DbInsert { table: String },
    #[serde(rename = "db_update")]
    DbUpdate { description: String },
    #[serde(rename = "event")]
    Event { name: String },
    #[serde(rename = "call")]
    ServiceCall { target: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authentication: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub roles: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limit: Option<String>,
}

/// A service-layer operation (non-HTTP).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Operation {
    pub name: String,
    pub source_method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<TypeRef>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub behaviors: Vec<Behavior>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction: Option<String>,
}

/// A database entity extracted from JPA/Hibernate/EF annotations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub name: String,
    pub table: String,
    pub fields: Vec<EntityField>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityField {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub constraints: Vec<String>,
}

/// A scheduled/cron task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledTask {
    pub name: String,
    pub schedule: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl ProjectSpec {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            analyzed: Some(chrono_now()),
            files_analyzed: 0,
            capabilities: Vec::new(),
        }
    }
}

fn chrono_now() -> String {
    // Simple ISO 8601 without chrono dependency
    "2026-03-25T00:00:00Z".to_string()
}

impl Capability {
    pub fn new(name: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            source: source.into(),
            endpoints: Vec::new(),
            operations: Vec::new(),
            entities: Vec::new(),
            scheduled_tasks: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.endpoints.is_empty()
            && self.operations.is_empty()
            && self.entities.is_empty()
            && self.scheduled_tasks.is_empty()
    }
}
