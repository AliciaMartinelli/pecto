use crate::model::ProjectSpec;

/// Serialize a ProjectSpec to YAML.
pub fn to_yaml(spec: &ProjectSpec) -> Result<String, serde_yaml::Error> {
    serde_yaml::to_string(spec)
}

/// Serialize a ProjectSpec to JSON.
pub fn to_json(spec: &ProjectSpec) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(spec)
}
