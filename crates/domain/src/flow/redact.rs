//! Flow definitions with data removed: test data (steps' `sample_output`), and for viewers also
//! every value a flow author typed (action inputs, variable values, condition operands, stop
//! messages, app trigger input), which can hold literal credentials such as an HTTP header.

use serde_json::{Map, Value};
use weft_expr::Condition;

use super::definition::{FlowDefinition, Step, StepKind, TriggerNode};

/// Keeps a condition's shape (operators, nesting) without its operands.
fn clear_condition(c: &mut Condition) {
    match c {
        Condition::All { all: items } | Condition::Any { any: items } => {
            items.iter_mut().for_each(clear_condition);
        }
        Condition::Not { not } => clear_condition(not),
        Condition::Leaf(leaf) => {
            leaf.left = Value::Null;
            leaf.right = Value::Null;
        }
    }
}

fn clear_values(kind: &mut StepKind) {
    match kind {
        StepKind::Action(a) => a.input = Value::Object(Map::new()),
        StepKind::SetVariable(v) => v.assignments.iter_mut().for_each(|a| a.value = Value::Null),
        StepKind::Stop(s) => s.message = None,
        StepKind::Decision(d) => d
            .branches
            .iter_mut()
            .for_each(|b| clear_condition(&mut b.condition)),
        StepKind::Delay(_) => {}
    }
}

fn clear(steps: &mut [Step], values: bool) {
    for step in steps {
        step.sample_output = None;
        if values {
            clear_values(&mut step.kind);
        }
        if let StepKind::Decision(d) = &mut step.kind {
            for branch in &mut d.branches {
                clear(&mut branch.steps, values);
            }
            clear(&mut d.otherwise, values);
        }
    }
}

impl FlowDefinition {
    /// The definition without test data (every step's `sample_output`).
    #[must_use]
    pub fn without_samples(&self) -> Self {
        let mut def = self.clone();
        clear(&mut def.steps, false);
        def
    }

    /// The definition as viewers see it: its structure, without test data or action input
    /// values.
    #[must_use]
    pub fn for_viewers(&self) -> Self {
        let mut def = self.clone();
        clear(&mut def.steps, true);
        if let TriggerNode::App(app) = &mut def.trigger {
            app.input = Value::Object(Map::new());
        }
        def
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn flow() -> FlowDefinition {
        serde_json::from_value(json!({
            "schema_version": 1,
            "trigger": {"type": "manual"},
            "steps": [
                {"key": "is_big", "type": "decision", "sample_output": {"secret": 1}, "branches": [
                    {"key": "big", "condition": {"left": "1", "op": "eq", "right": 1},
                     "steps": [{"key": "notify", "type": "action", "connector": "core.http", "action": "request",
                                "input": {"url": "https://example.com", "headers": {"Authorization": "Bearer x"}},
                                "sample_output": {"secret": 2}}]}
                ], "otherwise": [
                    {"key": "thank", "type": "action", "connector": "core.email", "action": "send",
                     "input": {"to": "a@example.com"}, "sample_output": {"secret": 3}}
                ]}
            ]
        }))
        .unwrap()
    }

    fn count(def: &FlowDefinition) -> (usize, usize) {
        let (mut samples, mut inputs) = (0, 0);
        def.walk(&mut |step, _| {
            samples += usize::from(step.sample_output.is_some());
            if let StepKind::Action(a) = &step.kind {
                inputs += a.input.as_object().map_or(1, Map::len);
            }
        });
        (samples, inputs)
    }

    #[test]
    fn viewers_see_no_typed_values_anywhere() {
        let def: FlowDefinition = serde_json::from_value(json!({
            "schema_version": 1,
            "trigger": {"type": "app", "connector": "memsrc", "trigger": "changed", "input": {"filter": "TRIGGER_SECRET"}},
            "steps": [
                {"key": "v", "type": "set_variable", "assignments": [{"name": "token", "value": "VAR_SECRET"}]},
                {"key": "d", "type": "decision", "branches": [{"key": "b",
                    "condition": {"any": [{"left": "{{ vars.token }}", "op": "eq", "right": "COND_SECRET"}]}, "steps": []}]},
                {"key": "s", "type": "stop", "status": "failed", "message": "STOP_SECRET"}
            ]
        }))
        .unwrap();
        let shown = serde_json::to_string(&def.for_viewers()).unwrap();
        for secret in ["TRIGGER_SECRET", "VAR_SECRET", "COND_SECRET", "STOP_SECRET"] {
            assert!(!shown.contains(secret), "{secret} in {shown}");
        }
        assert!(shown.contains("\"token\""), "variable names stay");
        assert!(
            shown.contains("\"op\":\"eq\""),
            "the condition's shape stays"
        );
    }

    #[test]
    fn test_data_and_viewer_inputs_are_cleared_in_every_branch() {
        let def = flow();
        assert_eq!(count(&def), (3, 3));
        assert_eq!(count(&def.without_samples()), (0, 3));
        let viewer = def.for_viewers();
        assert_eq!(count(&viewer), (0, 0));
        assert!(viewer.find_step("notify").is_some(), "the structure stays");
    }
}
