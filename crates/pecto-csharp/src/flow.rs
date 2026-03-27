use crate::context::AnalysisContext;
use pecto_core::model::*;

/// Extract request flows for C# endpoints.
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

            // Security
            if let Some(sec) = &endpoint.security
                && sec.authentication.is_some()
            {
                steps.push(FlowStep {
                    actor: cap.name.clone(),
                    method: "auth".to_string(),
                    kind: FlowStepKind::SecurityGuard,
                    description: "Auth required".to_string(),
                    condition: None,
                    children: Vec::new(),
                });
            }

            // Trace method body text for patterns
            trace_source_text(&file.source, &mut steps);

            // Return
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

fn trace_source_text(source: &str, steps: &mut Vec<FlowStep>) {
    for line in source.lines() {
        let trimmed = line.trim();

        if trimmed.contains(".Add(") || trimmed.contains(".SaveChangesAsync") {
            steps.push(FlowStep {
                actor: "DbContext".to_string(),
                method: "save".to_string(),
                kind: FlowStepKind::DbWrite,
                description: "DB write via EF Core".to_string(),
                condition: None,
                children: Vec::new(),
            });
        } else if trimmed.contains(".Remove(") {
            steps.push(FlowStep {
                actor: "DbContext".to_string(),
                method: "delete".to_string(),
                kind: FlowStepKind::DbWrite,
                description: "DB delete via EF Core".to_string(),
                condition: None,
                children: Vec::new(),
            });
        } else if trimmed.contains(".Publish(") || trimmed.contains(".Send(") {
            steps.push(FlowStep {
                actor: "Mediator".to_string(),
                method: "publish".to_string(),
                kind: FlowStepKind::EventPublish,
                description: "Publish event".to_string(),
                condition: None,
                children: Vec::new(),
            });
        }
    }
}
