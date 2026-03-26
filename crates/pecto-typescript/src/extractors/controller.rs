use super::common::*;
use crate::context::ParsedFile;
use pecto_core::model::*;

/// Extract endpoints from TypeScript/JavaScript files (Express, NestJS, Next.js).
pub fn extract(file: &ParsedFile) -> Option<Capability> {
    let root = file.tree.root_node();
    let source = file.source.as_bytes();
    let full_text = &file.source;

    let mut endpoints = Vec::new();

    // Strategy 1: NestJS decorators (@Controller, @Get, etc.)
    extract_nestjs_endpoints(&root, source, &mut endpoints);

    // Strategy 2: Express router/app calls (router.get, app.post, etc.)
    extract_express_endpoints(full_text, &mut endpoints);

    // Strategy 3: Next.js App Router (export function GET/POST in route.ts)
    if file.path.contains("route.") {
        extract_nextjs_endpoints(&root, source, file, &mut endpoints);
    }

    if endpoints.is_empty() {
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
    let capability_name = to_kebab_case(
        &file_stem
            .replace(".controller", "")
            .replace(".routes", "")
            .replace(".router", "")
            .replace("route", "api"),
    );

    let mut capability = Capability::new(capability_name, file.path.clone());
    capability.endpoints = endpoints;
    Some(capability)
}

// ==================== NestJS ====================

fn extract_nestjs_endpoints(
    _root: &tree_sitter::Node,
    source: &[u8],
    endpoints: &mut Vec<Endpoint>,
) {
    let full_text = std::str::from_utf8(source).unwrap_or("");
    // Quick check: does file contain NestJS patterns?
    if !full_text.contains("@Controller") && !full_text.contains("@Get") {
        return;
    }

    // Text-based NestJS extraction (reliable across tree-sitter versions)
    let base_path = full_text
        .split("@Controller(")
        .nth(1)
        .and_then(|s| s.split(')').next())
        .map(|s| clean_string_literal(s.trim()))
        .unwrap_or_default();

    // Find methods with @Get/@Post etc.
    let methods = [
        ("@Get(", HttpMethod::Get),
        ("@Post(", HttpMethod::Post),
        ("@Put(", HttpMethod::Put),
        ("@Delete(", HttpMethod::Delete),
        ("@Patch(", HttpMethod::Patch),
    ];

    for (marker, http_method) in &methods {
        let mut remaining = full_text;
        while let Some(pos) = remaining.find(marker) {
            let after = &remaining[pos + marker.len()..];
            let method_path = after
                .split(')')
                .next()
                .map(|s| clean_string_literal(s.trim()))
                .unwrap_or_default();

            let full_path = if base_path.is_empty() && method_path.is_empty() {
                "/".to_string()
            } else if method_path.is_empty() {
                format!("/{}", base_path)
            } else if base_path.is_empty() {
                format!("/{}", method_path.trim_start_matches('/'))
            } else {
                format!(
                    "/{}/{}",
                    base_path.trim_matches('/'),
                    method_path.trim_start_matches('/')
                )
            };

            let normalized = normalize_path_params(&full_path);

            // Check for @UseGuards nearby
            let security = if full_text.contains("@UseGuards") {
                Some(SecurityConfig {
                    authentication: Some("required".to_string()),
                    roles: Vec::new(),
                    rate_limit: None,
                    cors: None,
                })
            } else {
                None
            };

            endpoints.push(Endpoint {
                method: *http_method,
                path: normalized,
                input: None,
                validation: Vec::new(),
                behaviors: vec![Behavior {
                    name: "success".to_string(),
                    condition: None,
                    returns: ResponseSpec {
                        status: default_status(http_method),
                        body: None,
                    },
                    side_effects: Vec::new(),
                }],
                security,
            });

            remaining = &remaining[pos + marker.len()..];
        }
    }
}

// ==================== Express ====================

fn extract_express_endpoints(source: &str, endpoints: &mut Vec<Endpoint>) {
    for line in source.lines() {
        let trimmed = line.trim();

        // Match: router.get("/path", ...) or app.post("/path", ...)
        let method = if trimmed.contains(".get(") || trimmed.contains(".GET(") {
            Some(HttpMethod::Get)
        } else if trimmed.contains(".post(") || trimmed.contains(".POST(") {
            Some(HttpMethod::Post)
        } else if trimmed.contains(".put(") || trimmed.contains(".PUT(") {
            Some(HttpMethod::Put)
        } else if trimmed.contains(".delete(") || trimmed.contains(".DELETE(") {
            Some(HttpMethod::Delete)
        } else if trimmed.contains(".patch(") || trimmed.contains(".PATCH(") {
            Some(HttpMethod::Patch)
        } else {
            None
        };

        let Some(http_method) = method else {
            continue;
        };

        // Must look like a route definition (has a string path)
        if !trimmed.contains('"') && !trimmed.contains('\'') && !trimmed.contains('`') {
            continue;
        }

        // Skip non-route calls (e.g., array.get, map.delete)
        let has_route_prefix = trimmed.contains("router.")
            || trimmed.contains("app.")
            || trimmed.contains("route.")
            || trimmed.contains("server.");
        if !has_route_prefix {
            continue;
        }

        // Extract path string
        if let Some(path) = extract_string_arg(trimmed)
            && path.starts_with('/')
        {
            let normalized = normalize_path_params(&path);
            endpoints.push(Endpoint {
                method: http_method,
                path: normalized,
                input: None,
                validation: Vec::new(),
                behaviors: vec![Behavior {
                    name: "success".to_string(),
                    condition: None,
                    returns: ResponseSpec {
                        status: default_status(&http_method),
                        body: None,
                    },
                    side_effects: Vec::new(),
                }],
                security: None,
            });
        }
    }
}

// ==================== Next.js ====================

fn extract_nextjs_endpoints(
    root: &tree_sitter::Node,
    source: &[u8],
    file: &ParsedFile,
    endpoints: &mut Vec<Endpoint>,
) {
    // Derive path from file path: app/api/users/[id]/route.ts → /api/users/{id}
    let path = derive_nextjs_path(&file.path);

    // Find exported functions named GET, POST, PUT, DELETE, PATCH
    find_exported_functions(root, source, &path, endpoints);
}

fn derive_nextjs_path(file_path: &str) -> String {
    // Find "app/" or "src/app/" prefix
    let start = file_path.find("app/").map(|i| i + 4).unwrap_or(0);
    let path_part = &file_path[start..];

    // Remove route.ts/route.js
    let dir = path_part
        .rsplit('/')
        .skip(1)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("/");

    // Convert [param] to {param}
    let mut result = String::from("/");
    result.push_str(&dir.replace('[', "{").replace(']', "}"));
    result
}

fn find_exported_functions(
    node: &tree_sitter::Node,
    source: &[u8],
    path: &str,
    endpoints: &mut Vec<Endpoint>,
) {
    for i in 0..node.named_child_count() {
        let child = node.named_child(i).unwrap();

        if child.kind() == "export_statement" {
            let text = node_text(&child, source);
            let methods = [
                ("GET", HttpMethod::Get),
                ("POST", HttpMethod::Post),
                ("PUT", HttpMethod::Put),
                ("DELETE", HttpMethod::Delete),
                ("PATCH", HttpMethod::Patch),
            ];

            for (name, method) in &methods {
                if text.contains(&format!("function {}", name))
                    || text.contains(&format!("const {} ", name))
                    || text.contains(&format!("async function {}", name))
                {
                    endpoints.push(Endpoint {
                        method: *method,
                        path: path.to_string(),
                        input: None,
                        validation: Vec::new(),
                        behaviors: vec![Behavior {
                            name: "success".to_string(),
                            condition: None,
                            returns: ResponseSpec {
                                status: default_status(method),
                                body: None,
                            },
                            side_effects: Vec::new(),
                        }],
                        security: None,
                    });
                }
            }
        }
    }
}

// ==================== Helpers ====================

fn extract_string_arg(line: &str) -> Option<String> {
    // Find first string argument: "...", '...', or `...`
    for delim in ['"', '\'', '`'] {
        if let Some(start) = line.find(delim)
            && let Some(end) = line[start + 1..].find(delim)
        {
            return Some(line[start + 1..start + 1 + end].to_string());
        }
    }
    None
}

/// Convert Express :param to {param} for consistent output.
fn normalize_path_params(path: &str) -> String {
    let mut result = String::new();
    let mut chars = path.chars().peekable();
    while let Some(c) = chars.next() {
        if c == ':' {
            result.push('{');
            while let Some(&next) = chars.peek() {
                if next.is_alphanumeric() || next == '_' {
                    result.push(chars.next().unwrap());
                } else {
                    break;
                }
            }
            result.push('}');
        } else {
            result.push(c);
        }
    }
    result
}

fn default_status(method: &HttpMethod) -> u16 {
    match method {
        HttpMethod::Post => 201,
        HttpMethod::Delete => 204,
        _ => 200,
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
    fn test_express_routes() {
        let source = r#"
const express = require('express');
const router = express.Router();

router.get('/users', (req, res) => {
    res.json(users);
});

router.post('/users', (req, res) => {
    res.status(201).json(user);
});

router.get('/users/:id', (req, res) => {
    res.json(user);
});

router.delete('/users/:id', (req, res) => {
    res.status(204).send();
});

module.exports = router;
"#;

        let file = parse_file(source, "routes/users.ts");
        let capability = extract(&file).unwrap();

        assert_eq!(capability.endpoints.len(), 4);
        assert!(matches!(capability.endpoints[0].method, HttpMethod::Get));
        assert_eq!(capability.endpoints[0].path, "/users");
        assert_eq!(capability.endpoints[2].path, "/users/{id}");
        assert!(matches!(capability.endpoints[3].method, HttpMethod::Delete));
    }

    #[test]
    fn test_nestjs_controller() {
        let source = r#"
import { Controller, Get, Post, Param, Body, UseGuards } from '@nestjs/common';

@Controller('users')
export class UsersController {
    @Get()
    findAll() {
        return this.usersService.findAll();
    }

    @Get(':id')
    findOne(@Param('id') id: string) {
        return this.usersService.findOne(id);
    }

    @Post()
    @UseGuards(AuthGuard)
    create(@Body() dto: CreateUserDto) {
        return this.usersService.create(dto);
    }
}
"#;

        let file = parse_file(source, "users.controller.ts");
        let capability = extract(&file);

        assert!(
            capability.is_some(),
            "Should find NestJS controller capability"
        );
        let cap = capability.unwrap();
        assert!(
            cap.endpoints.len() >= 3,
            "Should find 3 endpoints, found {}",
            cap.endpoints.len()
        );
    }

    #[test]
    fn test_nextjs_route() {
        let source = r#"
import { NextResponse } from 'next/server';

export async function GET(request: Request) {
    return NextResponse.json({ users: [] });
}

export async function POST(request: Request) {
    const body = await request.json();
    return NextResponse.json(body, { status: 201 });
}
"#;

        let file = parse_file(source, "app/api/users/route.ts");
        let capability = extract(&file).unwrap();

        assert_eq!(capability.endpoints.len(), 2);
        assert!(matches!(capability.endpoints[0].method, HttpMethod::Get));
        assert_eq!(capability.endpoints[0].path, "/api/users");
        assert!(matches!(capability.endpoints[1].method, HttpMethod::Post));
    }

    #[test]
    fn test_nextjs_dynamic_route() {
        let source = r#"
export async function GET(request: Request, { params }: { params: { id: string } }) {
    return NextResponse.json({ id: params.id });
}

export async function DELETE(request: Request, { params }: { params: { id: string } }) {
    return new Response(null, { status: 204 });
}
"#;

        let file = parse_file(source, "src/app/api/users/[id]/route.ts");
        let capability = extract(&file).unwrap();

        assert_eq!(capability.endpoints.len(), 2);
        assert_eq!(capability.endpoints[0].path, "/api/users/{id}");
    }

    #[test]
    fn test_no_routes() {
        let source = r#"
export function helper() {
    return 42;
}

class Utils {
    static format(s: string) { return s; }
}
"#;
        let file = parse_file(source, "utils.ts");
        assert!(extract(&file).is_none());
    }
}
