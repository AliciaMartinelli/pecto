use crate::context::AnalysisContext;
use pecto_core::model::*;

/// Extract request flows for TypeScript endpoints.
pub fn extract_flows(spec: &mut ProjectSpec, ctx: &AnalysisContext) {
    let mut flows = Vec::new();

    for cap in &spec.capabilities {
        for endpoint in &cap.endpoints {
            let trigger = format!("{:?} {}", endpoint.method, endpoint.path);
            let entry_point = format!("{}#{}", cap.source, cap.name);

            let Some(file) = ctx.files.iter().find(|f| f.path == cap.source) else {
                continue;
            };

            let mut steps = Vec::new();

            if let Some(sec) = &endpoint.security
                && sec.authentication.is_some()
            {
                steps.push(FlowStep {
                    actor: cap.name.clone(),
                    method: "guard".to_string(),
                    kind: FlowStepKind::SecurityGuard,
                    description: "UseGuards(AuthGuard)".to_string(),
                    condition: None,
                    children: Vec::new(),
                });
            }

            let method_source = find_endpoint_source(&file.source, endpoint)
                .unwrap_or_else(|| file.source.clone());
            trace_source_text(&method_source, &mut steps);

            if let Some(b) = endpoint.behaviors.first() {
                steps.push(FlowStep {
                    actor: cap.name.clone(),
                    method: "return".to_string(),
                    kind: FlowStepKind::Return,
                    description: format!("Return: {}", b.returns.status),
                    condition: None,
                    children: Vec::new(),
                });
            }

            if steps.len() > 1 {
                flows.push(RequestFlow {
                    trigger,
                    entry_point,
                    steps,
                });
            }
        }
    }

    spec.flows = flows;
}

/// Find the source text of the specific method handling an endpoint.
fn find_endpoint_source(source: &str, endpoint: &Endpoint) -> Option<String> {
    let path_suffix = endpoint.path.rsplit('/').next().unwrap_or("");
    let method_lower = format!("{:?}", endpoint.method).to_lowercase();

    // NestJS: @Get('/path') or @Post('/path') followed by method body
    let decorator_patterns = [
        format!("@{}(", method_lower),                    // @get(
        format!("@{}(", capitalize(&method_lower)),       // @Get(
    ];

    // Express: router.get('/path', ...) or app.get('/path', ...)
    let express_patterns = [
        format!(".{}(", method_lower),  // .get(
    ];

    // Next.js: export function GET or export async function GET
    let method_upper = format!("{:?}", endpoint.method);
    let nextjs_patterns = [
        format!("function {}", method_upper),   // function GET
        format!("const {} ", method_upper),      // const GET
    ];

    let lines: Vec<&str> = source.lines().collect();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        // Check NestJS decorator match
        let is_nestjs = decorator_patterns.iter().any(|p| {
            trimmed.to_lowercase().contains(&p.to_lowercase())
                && (path_suffix.is_empty()
                    || trimmed.contains(path_suffix)
                    || trimmed.contains("()"))
        });

        // Check Express route match
        let is_express = express_patterns.iter().any(|p| {
            trimmed.contains(p) && trimmed.contains(&endpoint.path)
        });

        // Check Next.js export match
        let is_nextjs = nextjs_patterns.iter().any(|p| trimmed.contains(p));

        if is_nestjs || is_express || is_nextjs {
            // Find the opening brace and extract the function body
            return extract_brace_block(&lines, i);
        }
    }
    None
}

/// Extract text from opening `{` to matching `}` starting from line index.
fn extract_brace_block(lines: &[&str], start: usize) -> Option<String> {
    let mut depth = 0;
    let mut started = false;
    let mut result = Vec::new();

    for line in &lines[start..] {
        for ch in line.chars() {
            if ch == '{' {
                depth += 1;
                started = true;
            } else if ch == '}' {
                depth -= 1;
            }
        }
        if started {
            result.push(*line);
        }
        if started && depth == 0 {
            return Some(result.join("\n"));
        }
    }
    None
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

fn trace_source_text(source: &str, steps: &mut Vec<FlowStep>) {
    for line in source.lines() {
        let trimmed = line.trim();

        if trimmed.contains(".save(")
            || trimmed.contains(".create(") && trimmed.contains("repository")
        {
            steps.push(FlowStep {
                actor: "Repository".to_string(),
                method: "save".to_string(),
                kind: FlowStepKind::DbWrite,
                description: "DB write".to_string(),
                condition: None,
                children: Vec::new(),
            });
        } else if trimmed.contains(".find(") || trimmed.contains(".findOne(") {
            steps.push(FlowStep {
                actor: "Repository".to_string(),
                method: "find".to_string(),
                kind: FlowStepKind::DbRead,
                description: "DB read".to_string(),
                condition: None,
                children: Vec::new(),
            });
        } else if trimmed.contains(".emit(") || trimmed.contains(".publish(") {
            steps.push(FlowStep {
                actor: "EventBus".to_string(),
                method: "emit".to_string(),
                kind: FlowStepKind::EventPublish,
                description: "Emit event".to_string(),
                condition: None,
                children: Vec::new(),
            });
        }
    }
}
