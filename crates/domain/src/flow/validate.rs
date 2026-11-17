//! Publish-time validation. Drafts may be invalid; publishing requires zero errors.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use weft_expr::{Condition, check_template, referenced_steps};

use super::definition::{FlowDefinition, Step, StepKind, TriggerNode};
use crate::ids::ConnectionId;

/// Limits from `docs/plan/01-product-scope.md` §4.
pub const MAX_STEPS: usize = 100;
pub const MAX_DEPTH: usize = 8;
pub const MAX_DELAY_SECS: u64 = 30 * 24 * 3600;

/// What the validator needs to know about connectors (implemented by the connector registry).
pub trait Catalog {
    /// `None` when the connector or action does not exist.
    fn action(&self, connector: &str, action: &str) -> Option<ActionInfo>;
    /// `None` when the connector or trigger does not exist.
    fn trigger(&self, connector: &str, trigger: &str) -> Option<ActionInfo>;
}

/// The parts of an action or trigger spec that validation checks.
#[derive(Debug, Clone, Default)]
pub struct ActionInfo {
    pub requires_connection: bool,
    pub required_fields: Vec<String>,
}

/// Severity of a validation issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
}

/// One validation finding, located by JSON Pointer into the definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Issue {
    pub severity: Severity,
    pub path: String,
    pub code: String,
    pub message: String,
}

/// Returns true when any issue is an error.
#[must_use]
pub fn has_errors(issues: &[Issue]) -> bool {
    issues.iter().any(|i| i.severity == Severity::Error)
}

/// Validates a definition against the connector catalog.
#[must_use]
pub fn validate(def: &FlowDefinition, catalog: &dyn Catalog) -> Vec<Issue> {
    let mut v = Validator {
        catalog,
        issues: Vec::new(),
        all_keys: HashSet::new(),
        count: 0,
    };
    v.collect_keys(def);
    v.trigger(&def.trigger);
    if def.schema_version != super::definition::SCHEMA_VERSION {
        v.error(
            "/schema_version",
            "flow.schema_version",
            "Unsupported schema version",
        );
    }
    v.steps(&def.steps, "/steps", 1, &HashSet::new());
    if v.count > MAX_STEPS {
        v.error(
            "/steps",
            "flow.too_many_steps",
            &format!("A flow can have at most {MAX_STEPS} steps"),
        );
    }
    v.issues
}

struct Validator<'a> {
    catalog: &'a dyn Catalog,
    issues: Vec<Issue>,
    all_keys: HashSet<String>,
    count: usize,
}

impl Validator<'_> {
    fn push(&mut self, severity: Severity, path: &str, code: &str, message: &str) {
        self.issues.push(Issue {
            severity,
            path: path.to_owned(),
            code: code.to_owned(),
            message: message.to_owned(),
        });
    }

    fn error(&mut self, path: &str, code: &str, message: &str) {
        self.push(Severity::Error, path, code, message);
    }

    fn warning(&mut self, path: &str, code: &str, message: &str) {
        self.push(Severity::Warning, path, code, message);
    }

    fn collect_keys(&mut self, def: &FlowDefinition) {
        let mut seen: HashMap<String, String> = HashMap::new();
        let mut dupes = Vec::new();
        def.walk(&mut |step, path| {
            if seen.insert(step.key.clone(), path.to_owned()).is_some() {
                dupes.push((path.to_owned(), step.key.clone()));
            }
        });
        for (path, key) in dupes {
            self.error(
                &format!("{path}/key"),
                "step.duplicate_key",
                &format!("Step key `{key}` is used more than once"),
            );
        }
        self.all_keys = seen.into_keys().collect();
    }

    fn trigger(&mut self, trigger: &TriggerNode) {
        match trigger {
            TriggerNode::Webhook(_) | TriggerNode::Manual => {}
            TriggerNode::Schedule(s) => {
                if let Err(e) = s.schedule.validate() {
                    self.error("/trigger/schedule", "trigger.schedule_invalid", &e);
                }
            }
            TriggerNode::App(app) => match self.catalog.trigger(&app.connector, &app.trigger) {
                None => self.error(
                    "/trigger",
                    "trigger.unknown",
                    &format!("Unknown trigger `{}.{}`", app.connector, app.trigger),
                ),
                Some(info) => {
                    self.connection(
                        "/trigger/connection_id",
                        app.connection_id.as_deref(),
                        info.requires_connection,
                    );
                    self.required_fields("/trigger/input", &app.input, &info.required_fields);
                    self.templates("/trigger/input", &app.input, &HashSet::new());
                }
            },
        }
    }

    fn connection(&mut self, path: &str, id: Option<&str>, required: bool) {
        match id {
            Some(id) if !id.is_empty() => {
                if id.parse::<ConnectionId>().is_err() {
                    self.error(path, "connection.invalid", "Choose a connection");
                }
            }
            Some(_) | None => {
                if required {
                    self.error(path, "connection.required", "A connection is required");
                }
            }
        }
    }

    fn required_fields(&mut self, path: &str, input: &Value, required: &[String]) {
        for field in required {
            let missing = match input.get(field) {
                None | Some(Value::Null) => true,
                Some(Value::String(s)) => s.trim().is_empty(),
                Some(Value::Array(a)) => a.is_empty(),
                Some(Value::Bool(_) | Value::Number(_) | Value::Object(_)) => false,
            };
            if missing {
                self.error(
                    &format!("{path}/{field}"),
                    "field.required",
                    &format!("`{field}` is required"),
                );
            }
        }
    }

    /// Checks template syntax and `steps.<key>` references in every string leaf.
    fn templates(&mut self, path: &str, value: &Value, visible: &HashSet<String>) {
        match value {
            Value::String(s) => {
                if let Err(e) = check_template(s) {
                    self.error(path, "template.invalid", &e.to_string());
                }
                for key in referenced_steps(s) {
                    if !self.all_keys.contains(&key) {
                        self.error(
                            path,
                            "mapping.unknown_step",
                            &format!("Step `{key}` does not exist"),
                        );
                    } else if !visible.contains(&key) {
                        self.warning(
                            path,
                            "mapping.step_may_not_run",
                            &format!("Step `{key}` may not have run before this point"),
                        );
                    }
                }
            }
            Value::Array(items) => {
                for (i, item) in items.iter().enumerate() {
                    self.templates(&format!("{path}/{i}"), item, visible);
                }
            }
            Value::Object(map) => {
                for (k, item) in map {
                    self.templates(&format!("{path}/{}", escape(k)), item, visible);
                }
            }
            Value::Null | Value::Bool(_) | Value::Number(_) => {}
        }
    }

    fn condition(&mut self, path: &str, cond: &Condition, visible: &HashSet<String>) {
        match cond {
            Condition::All { all } => all
                .iter()
                .enumerate()
                .for_each(|(i, c)| self.condition(&format!("{path}/all/{i}"), c, visible)),
            Condition::Any { any } => any
                .iter()
                .enumerate()
                .for_each(|(i, c)| self.condition(&format!("{path}/any/{i}"), c, visible)),
            Condition::Not { not } => self.condition(&format!("{path}/not"), not, visible),
            Condition::Leaf(cmp) => {
                self.templates(&format!("{path}/left"), &cmp.left, visible);
                self.templates(&format!("{path}/right"), &cmp.right, visible);
            }
        }
    }

    /// `visible` holds keys of steps guaranteed to have run before this list starts.
    fn steps(&mut self, steps: &[Step], base: &str, depth: usize, visible: &HashSet<String>) {
        if depth > MAX_DEPTH {
            self.error(
                base,
                "flow.too_deep",
                &format!("Branches can be nested at most {MAX_DEPTH} levels"),
            );
            return;
        }
        let mut visible = visible.clone();
        for (i, step) in steps.iter().enumerate() {
            self.count += 1;
            let path = format!("{base}/{i}");
            if !valid_key(&step.key) {
                self.error(
                    &format!("{path}/key"),
                    "step.invalid_key",
                    "Keys use lowercase letters, digits and `_`, starting with a letter (max 64)",
                );
            }
            self.step(step, &path, depth, &visible);
            visible.insert(step.key.clone());
        }
    }

    fn step(&mut self, step: &Step, path: &str, depth: usize, visible: &HashSet<String>) {
        match &step.kind {
            StepKind::Action(a) => match self.catalog.action(&a.connector, &a.action) {
                None => self.error(
                    path,
                    "connector.action_not_found",
                    &format!("Unknown action `{}.{}`", a.connector, a.action),
                ),
                Some(info) => {
                    self.connection(
                        &format!("{path}/connection_id"),
                        a.connection_id.as_deref(),
                        info.requires_connection,
                    );
                    self.required_fields(&format!("{path}/input"), &a.input, &info.required_fields);
                    self.templates(&format!("{path}/input"), &a.input, visible);
                }
            },
            StepKind::Decision(d) => {
                if d.branches.is_empty() {
                    self.error(
                        &format!("{path}/branches"),
                        "decision.no_branches",
                        "Add at least one branch",
                    );
                }
                let mut branch_keys = HashSet::new();
                for (b, branch) in d.branches.iter().enumerate() {
                    let bpath = format!("{path}/branches/{b}");
                    if !valid_key(&branch.key) || !branch_keys.insert(branch.key.clone()) {
                        self.error(
                            &format!("{bpath}/key"),
                            "branch.invalid_key",
                            "Branch keys must be unique identifiers",
                        );
                    }
                    self.condition(&format!("{bpath}/condition"), &branch.condition, visible);
                    self.steps(&branch.steps, &format!("{bpath}/steps"), depth + 1, visible);
                }
                self.steps(
                    &d.otherwise,
                    &format!("{path}/otherwise"),
                    depth + 1,
                    visible,
                );
            }
            StepKind::Delay(delay) => match (delay.seconds, delay.until.as_deref()) {
                (Some(s), None) if s == 0 || s > MAX_DELAY_SECS => {
                    self.error(
                        &format!("{path}/seconds"),
                        "delay.out_of_range",
                        "Delay must be between 1 second and 30 days",
                    );
                }
                (Some(_), None) => {}
                (None, Some(until)) => self.templates(
                    &format!("{path}/until"),
                    &Value::String(until.to_owned()),
                    visible,
                ),
                (Some(_), Some(_)) | (None, None) => {
                    self.error(
                        path,
                        "delay.invalid",
                        "Set either a duration or a date-time",
                    );
                }
            },
            StepKind::SetVariable(sv) => {
                for (i, a) in sv.assignments.iter().enumerate() {
                    if !valid_key(&a.name) {
                        self.error(
                            &format!("{path}/assignments/{i}/name"),
                            "variable.invalid_name",
                            "Variable names use lowercase letters, digits and `_`",
                        );
                    }
                    self.templates(&format!("{path}/assignments/{i}/value"), &a.value, visible);
                }
            }
            StepKind::Stop(_) => {}
        }
    }
}

/// Step keys: `[a-z][a-z0-9_]{0,63}`.
#[must_use]
pub fn valid_key(key: &str) -> bool {
    let mut chars = key.chars();
    key.len() <= 64
        && chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn escape(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    struct TestCatalog;
    impl Catalog for TestCatalog {
        fn action(&self, connector: &str, action: &str) -> Option<ActionInfo> {
            match (connector, action) {
                ("core.http", "request") => Some(ActionInfo {
                    requires_connection: false,
                    required_fields: vec!["url".into()],
                }),
                ("zoho_crm", "create_record") => Some(ActionInfo {
                    requires_connection: true,
                    required_fields: vec!["module".into()],
                }),
                _ => None,
            }
        }
        fn trigger(&self, _: &str, _: &str) -> Option<ActionInfo> {
            None
        }
    }

    fn def(steps: Value) -> FlowDefinition {
        serde_json::from_value(json!({"trigger": {"type": "manual"}, "steps": steps})).unwrap()
    }

    fn codes(issues: &[Issue]) -> Vec<(String, String)> {
        issues
            .iter()
            .map(|i| (i.code.clone(), i.path.clone()))
            .collect()
    }

    #[test]
    fn valid_flow_has_no_issues() {
        let d = def(json!([
            {"key": "a", "type": "action", "connector": "core.http", "action": "request", "input": {"url": "https://x.io"}},
            {"key": "b", "type": "action", "connector": "core.http", "action": "request", "input": {"url": "https://x.io/{{ steps.a.body.id }}"}}
        ]));
        assert!(validate(&d, &TestCatalog).is_empty());
    }

    #[test]
    fn missing_fields_connections_and_unknown_actions_are_errors() {
        let d = def(json!([
            {"key": "a", "type": "action", "connector": "core.http", "action": "request", "input": {}},
            {"key": "b", "type": "action", "connector": "zoho_crm", "action": "create_record", "input": {"module": "Leads"}},
            {"key": "c", "type": "action", "connector": "nope", "action": "x"}
        ]));
        let issues = validate(&d, &TestCatalog);
        assert_eq!(
            codes(&issues),
            vec![
                ("field.required".into(), "/steps/0/input/url".into()),
                (
                    "connection.required".into(),
                    "/steps/1/connection_id".into()
                ),
                ("connector.action_not_found".into(), "/steps/2".into()),
            ]
        );
    }

    #[test]
    fn references_are_scope_checked() {
        let d = def(json!([
            {"key": "dec", "type": "decision", "branches": [
                {"key": "yes", "condition": {"left": "1", "op": "eq", "right": "1"}, "steps": [
                    {"key": "inner", "type": "set_variable", "assignments": [{"name": "x", "value": 1}]}
                ]}
            ]},
            {"key": "after", "type": "action", "connector": "core.http", "action": "request",
             "input": {"url": "{{ steps.inner.x }}{{ steps.ghost.y }}"}}
        ]));
        let issues = validate(&d, &TestCatalog);
        assert!(
            issues
                .iter()
                .any(|i| i.code == "mapping.step_may_not_run" && i.severity == Severity::Warning)
        );
        assert!(
            issues
                .iter()
                .any(|i| i.code == "mapping.unknown_step" && i.severity == Severity::Error)
        );
    }

    #[test]
    fn duplicate_and_invalid_keys_are_errors() {
        let d = def(json!([
            {"key": "a", "type": "stop"},
            {"key": "a", "type": "stop"},
            {"key": "Bad-Key", "type": "stop"}
        ]));
        let found: Vec<String> = validate(&d, &TestCatalog)
            .into_iter()
            .map(|i| i.code)
            .collect();
        assert!(found.contains(&"step.duplicate_key".to_owned()));
        assert!(found.contains(&"step.invalid_key".to_owned()));
    }

    #[test]
    fn delays_and_decisions_are_checked() {
        let d = def(json!([
            {"key": "d1", "type": "delay"},
            {"key": "d2", "type": "delay", "seconds": 99_999_999},
            {"key": "dec", "type": "decision", "branches": []},
            {"key": "t", "type": "action", "connector": "core.http", "action": "request", "input": {"url": "{{ bad. }}"}}
        ]));
        let found: Vec<String> = validate(&d, &TestCatalog)
            .into_iter()
            .map(|i| i.code)
            .collect();
        assert_eq!(
            found,
            vec![
                "delay.invalid",
                "delay.out_of_range",
                "decision.no_branches",
                "template.invalid"
            ]
        );
    }

    #[test]
    fn key_rules() {
        assert!(valid_key("send_email_2"));
        assert!(!valid_key("2send"));
        assert!(!valid_key(""));
        assert!(!valid_key("Send"));
    }
}
