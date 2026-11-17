//! Unit tests for `validate`.

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
