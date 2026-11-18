//! Flow definitions with data removed: test data (steps' `sample_output`), and for viewers also
//! the action input values, which can hold literal credentials such as an HTTP header.

use serde_json::{Map, Value};

use super::definition::{FlowDefinition, Step, StepKind};

fn clear(steps: &mut [Step], inputs: bool) {
    for step in steps {
        step.sample_output = None;
        if inputs && let StepKind::Action(a) = &mut step.kind {
            a.input = Value::Object(Map::new());
        }
        if let StepKind::Decision(d) = &mut step.kind {
            for branch in &mut d.branches {
                clear(&mut branch.steps, inputs);
            }
            clear(&mut d.otherwise, inputs);
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
    fn test_data_and_viewer_inputs_are_cleared_in_every_branch() {
        let def = flow();
        assert_eq!(count(&def), (3, 3));
        assert_eq!(count(&def.without_samples()), (0, 3));
        let viewer = def.for_viewers();
        assert_eq!(count(&viewer), (0, 0));
        assert!(viewer.find_step("notify").is_some(), "the structure stays");
    }
}
