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

            trace_source_text(&file.source, &mut steps);

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
