use crate::model::{FlowStep, FlowStepKind, RequestFlow};

/// Convert a RequestFlow into a Mermaid Sequence Diagram.
pub fn flow_to_mermaid(flow: &RequestFlow) -> String {
    let mut out = String::new();
    out.push_str("sequenceDiagram\n");

    // Collect unique actors from steps
    let mut actors = Vec::new();
    actors.push("Client".to_string());
    collect_actors(&flow.steps, &mut actors);

    // Add participants
    for actor in &actors {
        out.push_str(&format!("    participant {}\n", sanitize_actor(actor)));
    }
    out.push('\n');

    // Entry arrow
    let entry_actor = flow
        .steps
        .first()
        .map(|s| sanitize_actor(&s.actor))
        .unwrap_or_else(|| "Server".to_string());
    out.push_str(&format!(
        "    Client->>{}:{}\n",
        entry_actor,
        sanitize_label(&flow.trigger)
    ));

    // Render steps
    let mut last_actor = entry_actor.clone();
    render_steps(&flow.steps, &mut last_actor, &mut out, 1);

    // Return arrow
    out.push_str(&format!("    {}-->>Client: Response\n", last_actor));

    out
}

fn collect_actors(steps: &[FlowStep], actors: &mut Vec<String>) {
    for step in steps {
        let actor = sanitize_actor(&step.actor);
        if !actor.is_empty() && !actors.contains(&actor) {
            actors.push(actor);
        }
        collect_actors(&step.children, actors);
    }
}

fn render_steps(steps: &[FlowStep], last_actor: &mut String, out: &mut String, depth: usize) {
    if depth > 6 {
        return; // Prevent infinite recursion
    }

    for step in steps {
        let actor = if step.actor.is_empty() {
            last_actor.clone()
        } else {
            sanitize_actor(&step.actor)
        };

        match step.kind {
            FlowStepKind::ServiceCall => {
                out.push_str(&format!(
                    "    {}->>{}:{}\n",
                    last_actor,
                    actor,
                    sanitize_label(&step.description)
                ));
                if !step.children.is_empty() {
                    let mut child_actor = actor.clone();
                    render_steps(&step.children, &mut child_actor, out, depth + 1);
                }
                *last_actor = actor;
            }
            FlowStepKind::DbWrite => {
                out.push_str(&format!(
                    "    {}->>{}:💾 {}\n",
                    last_actor,
                    actor,
                    sanitize_label(&step.description)
                ));
            }
            FlowStepKind::DbRead => {
                out.push_str(&format!(
                    "    {}->>{}:🔍 {}\n",
                    last_actor,
                    actor,
                    sanitize_label(&step.description)
                ));
                out.push_str(&format!("    {}-->>{}:result\n", actor, last_actor));
            }
            FlowStepKind::EventPublish => {
                let event_actor = if actor.is_empty() {
                    "EventBus".to_string()
                } else {
                    actor.clone()
                };
                out.push_str(&format!(
                    "    {}->>{}:📢 {}\n",
                    last_actor,
                    event_actor,
                    sanitize_label(&step.description)
                ));
            }
            FlowStepKind::Validation => {
                out.push_str(&format!(
                    "    Note over {}:✅ {}\n",
                    last_actor,
                    sanitize_label(&step.description)
                ));
            }
            FlowStepKind::SecurityGuard => {
                out.push_str(&format!(
                    "    Note over {}:🔒 {}\n",
                    last_actor,
                    sanitize_label(&step.description)
                ));
            }
            FlowStepKind::Condition => {
                let cond = step.condition.as_deref().unwrap_or(&step.description);
                if cond == "else" {
                    out.push_str(&format!("    else {}\n", sanitize_label(cond)));
                } else {
                    out.push_str(&format!("    alt {}\n", sanitize_label(cond)));
                }
                if !step.children.is_empty() {
                    render_steps(&step.children, last_actor, out, depth + 1);
                }
                // Only close alt if this is the last condition or if next is not an else
                if cond != "else" && !has_else_sibling(steps, step) {
                    out.push_str("    end\n");
                }
            }
            FlowStepKind::ThrowException => {
                out.push_str(&format!(
                    "    {}-->>Client:❌ {}\n",
                    last_actor,
                    sanitize_label(&step.description)
                ));
            }
            FlowStepKind::Return => {
                // Return is handled at the end
            }
        }
    }
}

fn has_else_sibling(steps: &[FlowStep], current: &FlowStep) -> bool {
    let mut found_current = false;
    for step in steps {
        if std::ptr::eq(step, current) {
            found_current = true;
            continue;
        }
        if found_current
            && matches!(step.kind, FlowStepKind::Condition)
            && step.condition.as_deref() == Some("else")
        {
            return true;
        }
        if found_current {
            break;
        }
    }
    false
}

/// Sanitize actor name for Mermaid (no spaces, special chars).
fn sanitize_actor(name: &str) -> String {
    if name.is_empty() {
        return "Server".to_string();
    }
    let mut result = String::with_capacity(name.len());
    for c in name.chars() {
        match c {
            ' ' | '.' | '-' => result.push('_'),
            '(' | ')' => {}
            _ => result.push(c),
        }
    }
    result
}

/// Sanitize label text for Mermaid.
fn sanitize_label(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '"' => result.push('\''),
            '\n' => result.push(' '),
            ';' => result.push(','),
            _ => result.push(c),
        }
    }
    result.chars().take(80).collect()
}

/// Convert all flows in a ProjectSpec to Mermaid diagrams.
pub fn all_flows_to_mermaid(flows: &[RequestFlow]) -> Vec<(String, String)> {
    flows
        .iter()
        .map(|f| (f.trigger.clone(), flow_to_mermaid(f)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    #[test]
    fn test_simple_flow_to_mermaid() {
        let flow = RequestFlow {
            trigger: "POST /api/orders".to_string(),
            entry_point: "OrderController#create".to_string(),
            steps: vec![
                FlowStep {
                    actor: "OrderController".to_string(),
                    method: "validate".to_string(),
                    kind: FlowStepKind::Validation,
                    description: "Validate: @Valid CreateOrderRequest".to_string(),
                    condition: None,
                    children: Vec::new(),
                },
                FlowStep {
                    actor: "orderService".to_string(),
                    method: "create".to_string(),
                    kind: FlowStepKind::ServiceCall,
                    description: "Call: orderService.create()".to_string(),
                    condition: None,
                    children: vec![
                        FlowStep {
                            actor: "orderRepository".to_string(),
                            method: "save".to_string(),
                            kind: FlowStepKind::DbWrite,
                            description: "DB write: orderRepository".to_string(),
                            condition: None,
                            children: Vec::new(),
                        },
                        FlowStep {
                            actor: "EventBus".to_string(),
                            method: "publish".to_string(),
                            kind: FlowStepKind::EventPublish,
                            description: "Publish: OrderCreatedEvent".to_string(),
                            condition: None,
                            children: Vec::new(),
                        },
                    ],
                },
                FlowStep {
                    actor: "OrderController".to_string(),
                    method: "return".to_string(),
                    kind: FlowStepKind::Return,
                    description: "Return: 201 Order".to_string(),
                    condition: None,
                    children: Vec::new(),
                },
            ],
        };

        let mermaid = flow_to_mermaid(&flow);
        assert!(mermaid.contains("sequenceDiagram"));
        assert!(mermaid.contains("Client->>"));
        assert!(mermaid.contains("orderService"));
        assert!(mermaid.contains("DB write"));
        assert!(mermaid.contains("Publish"));
    }
}
