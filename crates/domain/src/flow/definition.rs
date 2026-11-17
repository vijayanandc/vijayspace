//! The flow definition document (schema v1): one trigger plus a tree of steps.
//! See `docs/plan/04-workflow-engine.md` §2.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use weft_expr::Condition;

use crate::schedule::ScheduleSpec;

/// Current schema version written by this build.
pub const SCHEMA_VERSION: u16 = 1;

const fn schema_version() -> u16 {
    SCHEMA_VERSION
}

/// A complete flow: trigger + steps. Drafts may be invalid; publishing requires `validate` to pass.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowDefinition {
    #[serde(default = "schema_version")]
    pub schema_version: u16,
    pub trigger: TriggerNode,
    #[serde(default)]
    pub steps: Vec<Step>,
}

impl FlowDefinition {
    /// A new definition with the given trigger and no steps.
    #[must_use]
    pub const fn new(trigger: TriggerNode) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            trigger,
            steps: Vec::new(),
        }
    }

    /// Visits every step depth-first (pre-order) with its JSON Pointer path.
    pub fn walk<'a>(&'a self, f: &mut dyn FnMut(&'a Step, &str)) {
        walk_steps(&self.steps, "/steps", f);
    }

    /// Finds a step anywhere in the tree by key.
    #[must_use]
    pub fn find_step(&self, key: &str) -> Option<&Step> {
        let mut found = None;
        self.walk(&mut |step, _| {
            if found.is_none() && step.key == key {
                found = Some(step);
            }
        });
        found
    }
}

fn walk_steps<'a>(steps: &'a [Step], base: &str, f: &mut dyn FnMut(&'a Step, &str)) {
    for (i, step) in steps.iter().enumerate() {
        let path = format!("{base}/{i}");
        f(step, &path);
        if let StepKind::Decision(d) = &step.kind {
            for (b, branch) in d.branches.iter().enumerate() {
                walk_steps(&branch.steps, &format!("{path}/branches/{b}/steps"), f);
            }
            walk_steps(&d.otherwise, &format!("{path}/otherwise"), f);
        }
    }
}

/// What starts an execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TriggerNode {
    /// An HTTP request to the flow's unique URL.
    Webhook(WebhookTrigger),
    /// Time based: once, every N minutes, or cron in a time zone.
    Schedule(ScheduleTrigger),
    /// An event in a connected app (instant subscription or polling).
    App(AppTrigger),
    /// Only started by "Run now" or the API.
    Manual,
}

/// Options for webhook triggers.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct WebhookTrigger {
    /// A JSON array body starts one execution per element.
    #[serde(default)]
    pub split_array: bool,
    /// Header whose value deduplicates requests (default `Idempotency-Key`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_header: Option<String>,
    /// If set, requests must carry this header with the registration's secret.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_header: Option<String>,
}

/// Options for schedule triggers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduleTrigger {
    pub schedule: ScheduleSpec,
}

/// Options for app triggers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppTrigger {
    pub connector: String,
    pub trigger: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
    #[serde(default)]
    pub input: Value,
    /// Polling interval for polling triggers and the reconciliation poll of instant ones.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poll_interval_secs: Option<u32>,
    /// Collapse events for the same record within this window into one execution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debounce_secs: Option<u32>,
}

/// One node of the flow.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Step {
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(flatten)]
    pub kind: StepKind,
    #[serde(default, skip_serializing_if = "OnError::is_default")]
    pub on_error: OnError,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<RetryOverride>,
    /// Builder-only: output captured by "Test step", used for variable pickers. Ignored by the engine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_output: Option<Value>,
}

/// The kinds of steps. Everything that talks to the outside world is an `Action` of a connector.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StepKind {
    Action(ActionStep),
    Decision(DecisionStep),
    Delay(DelayStep),
    SetVariable(SetVariableStep),
    Stop(StopStep),
}

impl StepKind {
    /// Stored kind name (`action`, `decision`, …).
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Action(_) => "action",
            Self::Decision(_) => "decision",
            Self::Delay(_) => "delay",
            Self::SetVariable(_) => "set_variable",
            Self::Stop(_) => "stop",
        }
    }
}

/// Run one action of a connector with templated input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionStep {
    pub connector: String,
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
    #[serde(default)]
    pub input: Value,
}

/// Multi-branch decision; the first matching branch runs, otherwise `otherwise`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionStep {
    pub branches: Vec<Branch>,
    #[serde(default)]
    pub otherwise: Vec<Step>,
}

/// One branch of a decision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Branch {
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub condition: Condition,
    #[serde(default)]
    pub steps: Vec<Step>,
}

/// Pause for a duration, or until a (templated) date-time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DelayStep {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<String>,
}

/// Assign flow variables (`vars.<name>`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetVariableStep {
    pub assignments: Vec<Assignment>,
}

/// One variable assignment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Assignment {
    pub name: String,
    pub value: Value,
}

/// End the execution early.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct StopStep {
    #[serde(default)]
    pub status: StopStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Final status chosen by a stop step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopStatus {
    #[default]
    Succeeded,
    Failed,
}

/// What happens when an action fails permanently.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnError {
    #[default]
    FailFlow,
    Continue,
}

impl OnError {
    #[allow(clippy::trivially_copy_pass_by_ref)] // serde's skip_serializing_if passes a reference
    fn is_default(&self) -> bool {
        *self == Self::FailFlow
    }
}

/// Per-step override of the retry policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetryOverride {
    pub max_attempts: u16,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    pub(crate) fn order_alert() -> Value {
        json!({
            "schema_version": 1,
            "trigger": {"type": "webhook", "split_array": true},
            "steps": [
                {"key": "is_big", "type": "decision", "branches": [
                    {"key": "big", "condition": {"left": "{{ trigger.body.amount }}", "op": "gt", "right": 100},
                     "steps": [{"key": "notify", "type": "action", "connector": "core.http", "action": "request",
                                "input": {"method": "POST", "url": "https://example.com/hook"}}]}
                ], "otherwise": [
                    {"key": "thank", "type": "action", "connector": "core.email", "action": "send",
                     "input": {"to": "{{ trigger.body.email }}"}, "on_error": "continue"}
                ]},
                {"key": "wait", "type": "delay", "seconds": 86400},
                {"key": "mark", "type": "set_variable", "assignments": [{"name": "done", "value": true}]}
            ]
        })
    }

    #[test]
    fn definition_round_trips_through_json() {
        let raw = order_alert();
        let def: FlowDefinition = serde_json::from_value(raw.clone()).unwrap();
        assert_eq!(serde_json::to_value(&def).unwrap(), raw);
        assert!(def.find_step("thank").is_some());
    }

    #[test]
    fn walk_reports_json_pointer_paths() {
        let def: FlowDefinition = serde_json::from_value(order_alert()).unwrap();
        let mut paths = Vec::new();
        def.walk(&mut |s, p| paths.push(format!("{}={p}", s.key)));
        assert_eq!(
            paths,
            vec![
                "is_big=/steps/0",
                "notify=/steps/0/branches/0/steps/0",
                "thank=/steps/0/otherwise/0",
                "wait=/steps/1",
                "mark=/steps/2",
            ]
        );
    }
}
