use super::common::*;
use crate::context::ParsedFile;
use pecto_core::model::*;
use std::collections::BTreeMap;

/// Extract consumed REST endpoints from C# client code using RestSharp or HttpClient patterns.
pub fn extract(file: &ParsedFile) -> Option<Capability> {
    let full_text = &file.source;

    // Quick check: does this file use RestSharp or HttpClient?
    if !full_text.contains("RestRequest")
        && !full_text.contains("RestClient")
        && !full_text.contains("HttpClient")
    {
        return None;
    }

    let mut endpoints = Vec::new();

    // Extract const string paths: private const string XxxPath = "/some/path/{param}";
    let base_path = extract_base_path(full_text);
    let sub_paths = extract_sub_paths(full_text);

    // Scan for RestRequest/AscoRestRequest constructor calls to find HTTP methods
    let calls = extract_rest_calls(full_text, &base_path, &sub_paths);

    for (method, path) in calls {
        let mut path_params = Vec::new();

        // Extract {param} from path
        let mut remaining = path.as_str();
        while let Some(start) = remaining.find('{') {
            if let Some(end) = remaining[start..].find('}') {
                let param_name = &remaining[start + 1..start + end];
                if !param_name.is_empty() {
                    path_params.push(Param {
                        name: param_name.to_string(),
                        param_type: "string".to_string(),
                        required: true,
                    });
                }
                remaining = &remaining[start + end + 1..];
            } else {
                break;
            }
        }

        let input = if path_params.is_empty() {
            None
        } else {
            Some(EndpointInput {
                body: None,
                path_params,
                query_params: Vec::new(),
            })
        };

        // Check if POST has body
        let has_body = method == HttpMethod::Post || method == HttpMethod::Put;
        let input = if has_body && input.is_none() {
            Some(EndpointInput {
                body: Some(TypeRef {
                    name: "JSON".to_string(),
                    fields: BTreeMap::new(),
                }),
                path_params: Vec::new(),
                query_params: Vec::new(),
            })
        } else {
            input
        };

        endpoints.push(Endpoint {
            method,
            path: path.clone(),
            input,
            validation: Vec::new(),
            behaviors: vec![Behavior {
                name: "success".to_string(),
                condition: None,
                returns: ResponseSpec {
                    status: 200,
                    body: None,
                },
                side_effects: Vec::new(),
            }],
            security: None,
        });
    }

    if endpoints.is_empty() {
        return None;
    }

    // Derive capability name from file
    let class_name = extract_class_name_from_source(full_text);
    let capability_name = to_kebab_case(
        &class_name
            .replace("Service", "")
            .replace("Client", "")
            .replace("Api", ""),
    );

    let mut capability = Capability::new(format!("{}-client", capability_name), file.path.clone());
    capability.endpoints = endpoints;
    Some(capability)
}

/// Extract the base REST service path: `const string RestServicePath = "bid"`
fn extract_base_path(source: &str) -> String {
    for line in source.lines() {
        let trimmed = line.trim();
        if (trimmed.contains("RestServicePath")
            || trimmed.contains("REST_SERVICE_PATH")
            || trimmed.contains("BasePath"))
            && trimmed.contains("const")
            && trimmed.contains('"')
            && let Some(val) = extract_string_value(trimmed)
        {
            return val;
        }
    }
    String::new()
}

/// Extract sub-path constants: `const string GetAllPath = "/getAll"`
fn extract_sub_paths(source: &str) -> Vec<(String, String)> {
    let mut paths = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.contains("const")
            && trimmed.contains("string")
            && trimmed.contains("Path")
            && trimmed.contains('"')
            && trimmed.contains('/')
            && !trimmed.contains("RestServicePath")
            && !trimmed.contains("REST_SERVICE")
            && !trimmed.contains("BasePath")
        {
            // Extract the constant name and value
            if let Some(name) = extract_const_name(trimmed)
                && let Some(val) = extract_string_value(trimmed)
            {
                paths.push((name, val));
            }
        }
    }
    paths
}

/// Extract REST calls by finding request constructors and matching them with paths.
fn extract_rest_calls(
    source: &str,
    base_path: &str,
    sub_paths: &[(String, String)],
) -> Vec<(HttpMethod, String)> {
    let mut calls = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for line in source.lines() {
        let trimmed = line.trim();

        // Match: new AscoRestRequest(uri, Method.POST) or new RestRequest(uri, Method.GET)
        if (trimmed.contains("RestRequest(") || trimmed.contains("RestRequest ("))
            && !trimmed.starts_with("//")
        {
            let method = if trimmed.contains("Method.POST") || trimmed.contains("Method.Post") {
                HttpMethod::Post
            } else if trimmed.contains("Method.PUT") || trimmed.contains("Method.Put") {
                HttpMethod::Put
            } else if trimmed.contains("Method.DELETE") || trimmed.contains("Method.Delete") {
                HttpMethod::Delete
            } else if trimmed.contains("Method.PATCH") || trimmed.contains("Method.Patch") {
                HttpMethod::Patch
            } else {
                HttpMethod::Get
            };

            // Try to find the URI variable or string used in the constructor
            let path = resolve_uri_from_line(trimmed, source, base_path, sub_paths);
            if !path.is_empty() && seen.insert(format!("{:?}{}", method, path)) {
                calls.push((method, path));
            }
        }
    }

    // Also look for string concatenations: const string uri = RestServicePath + SomePath
    for line in source.lines() {
        let trimmed = line.trim();
        if (trimmed.contains("uri =") || trimmed.contains("URI ="))
            && trimmed.contains('+')
            && !trimmed.starts_with("//")
        {
            let path = resolve_concatenated_path(trimmed, base_path, sub_paths);
            if !path.is_empty() && !seen.contains(&path) {
                // Find the method in nearby lines (look ahead for RestRequest)
                seen.insert(path.clone());
                // Default to GET, will be corrected if Method.POST is found nearby
                if !calls.iter().any(|(_, p)| p == &path) {
                    calls.push((HttpMethod::Get, path));
                }
            }
        }
    }

    calls
}

fn resolve_uri_from_line(
    line: &str,
    source: &str,
    base_path: &str,
    sub_paths: &[(String, String)],
) -> String {
    // Check if the URI is a direct string in the constructor
    if let Some(start) = line.find("Request(") {
        let after = &line[start + 8..];
        if after.starts_with('"')
            && let Some(val) = extract_first_string(after)
        {
            return val;
        }

        // It's a variable reference — find the variable definition
        let var_name = after.split([',', ')']).next().unwrap_or("").trim();

        if !var_name.is_empty() {
            // Search for: const string <var> = ... or string <var> = ...
            return find_variable_value(var_name, source, base_path, sub_paths);
        }
    }

    String::new()
}

fn find_variable_value(
    var_name: &str,
    source: &str,
    base_path: &str,
    sub_paths: &[(String, String)],
) -> String {
    for line in source.lines() {
        let trimmed = line.trim();
        // Match: const string uri = "path" or string uri = RestServicePath + SubPath
        if trimmed.contains(var_name)
            && trimmed.contains('=')
            && !trimmed.contains("RestRequest")
            && let Some(eq_pos) = trimmed.find('=')
        {
            let rhs = trimmed[eq_pos + 1..].trim().trim_end_matches(';');

            // Direct string
            if rhs.starts_with('"') {
                return extract_first_string(rhs).unwrap_or_default();
            }

            // Concatenation: RestServicePath + SomePath
            return resolve_concatenated_path(trimmed, base_path, sub_paths);
        }
    }
    String::new()
}

fn resolve_concatenated_path(
    line: &str,
    base_path: &str,
    sub_paths: &[(String, String)],
) -> String {
    let eq_pos = match line.find('=') {
        Some(p) => p,
        None => return String::new(),
    };
    let rhs = line[eq_pos + 1..].trim().trim_end_matches(';');

    let parts: Vec<&str> = rhs.split('+').map(|s| s.trim()).collect();
    let mut result = String::new();

    for part in parts {
        let part = part.trim_matches('"');
        if part.contains("RestServicePath")
            || part.contains("REST_SERVICE_PATH")
            || part.contains("BasePath")
        {
            result.push_str(base_path);
        } else if let Some((_, val)) = sub_paths
            .iter()
            .find(|(name, _)| part.contains(name.as_str()))
        {
            result.push_str(val);
        } else if part.starts_with('/') || part.starts_with('{') {
            result.push_str(part);
        } else if !part.is_empty()
            && !part.contains('(')
            && !part.contains("string")
            && !part.contains("const")
        {
            // Unknown reference — try as literal
            result.push_str(part);
        }
    }

    result
}

fn extract_string_value(line: &str) -> Option<String> {
    let start = line.find('"')? + 1;
    let end = line[start..].find('"')? + start;
    Some(line[start..end].to_string())
}

fn extract_first_string(text: &str) -> Option<String> {
    let inner = text.strip_prefix('"')?;
    let end = inner.find('"')?;
    Some(inner[..end].to_string())
}

fn extract_const_name(line: &str) -> Option<String> {
    // "private const string GetAllPath = ..." → "GetAllPath"
    let parts: Vec<&str> = line.split_whitespace().collect();
    for (i, p) in parts.iter().enumerate() {
        if *p == "string" && i + 1 < parts.len() {
            return Some(parts[i + 1].trim_end_matches('=').to_string());
        }
    }
    None
}

fn extract_class_name_from_source(source: &str) -> String {
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.contains("class ") && (trimmed.contains("public") || trimmed.contains("static"))
        {
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            for (i, p) in parts.iter().enumerate() {
                if *p == "class" && i + 1 < parts.len() {
                    return parts[i + 1]
                        .trim_end_matches(':')
                        .trim_end_matches('{')
                        .to_string();
                }
            }
        }
    }
    "unknown".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ParsedFile;

    fn parse_file(source: &str, path: &str) -> ParsedFile {
        ParsedFile::parse(source.to_string(), path.to_string()).unwrap()
    }

    #[test]
    fn test_restsharp_client_extraction() {
        let source = r#"
using RestSharp;

public static class AccountService
{
    private const string RestServicePath = "account";
    private const string GetAccountPath = "/getAccount";
    private const string LoginAuditPath = "/loginAuditEvent/{username}/{ip}/{machine}";

    public static Account GetAccount()
    {
        const string uri = RestServicePath + GetAccountPath;
        RestRequest request = new AscoRestRequest(uri);
        IRestResponse<Account> response = RestServiceClient.Instance.ExecuteWithException<Account>(request, "Error");
        return response.Data;
    }

    public static void PostAuditEvent()
    {
        const string uri = RestServicePath + LoginAuditPath;
        RestRequest request = new AscoRestRequest(uri, Method.POST);
        request.AddUrlSegment("username", ClientInfo.Instance.User);
        RestServiceClient.Instance.ExecuteWithException(request, "error", true);
    }
}
"#;

        let file = parse_file(source, "Services/AccountService.cs");
        let capability = extract(&file).unwrap();

        assert_eq!(capability.name, "account-client");
        assert!(
            capability.endpoints.len() >= 2,
            "Should find at least 2 endpoints, found {}",
            capability.endpoints.len()
        );

        // GET account/getAccount
        assert!(
            capability
                .endpoints
                .iter()
                .any(|e| matches!(e.method, HttpMethod::Get))
        );

        // POST with path params
        assert!(
            capability
                .endpoints
                .iter()
                .any(|e| matches!(e.method, HttpMethod::Post))
        );
    }

    #[test]
    fn test_no_rest_client() {
        let source = r#"
public class Calculator
{
    public int Add(int a, int b) { return a + b; }
}
"#;

        let file = parse_file(source, "Calculator.cs");
        assert!(extract(&file).is_none());
    }
}
