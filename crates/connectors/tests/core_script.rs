//! `core.script`: publish-time checks (syntax with line/column, template markers, language) and
//! execution through a `ScriptRunner` (input values, context, `_logs`, coded failures).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use weft_connector_sdk::{Connector, ConnectorError, ScriptRun, ScriptRunner, testing::ctx};
use weft_connectors::core::script::ScriptConnector;
use weft_http::{ReqwestSender, SsrfPolicy};
use weft_script::Limits;

/// Runs scripts inline (the server uses a pooled runner with the same limits).
struct InlineRunner;

#[async_trait]
impl ScriptRunner for InlineRunner {
    async fn run(
        &self,
        code: &str,
        input: Value,
        context: Value,
    ) -> Result<ScriptRun, ConnectorError> {
        weft_script::run(code, &input, &context, &Limits::default())
            .map(|o| ScriptRun {
                output: o.value,
                logs: o.logs,
            })
            .map_err(|e| ConnectorError::Coded {
                code: e.code(),
                message: e.to_string(),
                details: Value::Null,
            })
    }
}

fn script_ctx() -> weft_connector_sdk::ActionCtx {
    let mut cx = ctx(
        Arc::new(ReqwestSender::new(SsrfPolicy::public_only()).unwrap()),
        None,
    );
    cx.scripts = Some(Arc::new(InlineRunner));
    cx
}

#[test]
fn syntax_errors_template_markers_and_languages_are_reported_at_validation() {
    let c = ScriptConnector::new();
    let issues = c.check_input("run", &json!({"code": "let x = 1;\nlet y = ;"}));
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].0, "code");
    assert!(issues[0].1.starts_with("line 2"), "{issues:?}");
    let markers = c.check_input("run", &json!({"code": "#{ a: \"{{ x }}\" }"}));
    assert!(markers[0].1.contains("{{"), "{markers:?}");
    let lang = c.check_input("run", &json!({"code": "1", "language": "python"}));
    assert_eq!(lang[0].0, "language");
    assert!(
        c.check_input("run", &json!({"code": "#{ a: input.x }"}))
            .is_empty()
    );
    assert!(
        c.check_input("run", &json!({})).is_empty(),
        "missing code is the generic required check"
    );
}

#[tokio::test]
async fn runs_with_input_values_and_context_and_keeps_print_lines() {
    let c = ScriptConnector::new();
    let out = c
        .execute(&script_ctx(), "run", json!({
            "code": "print(`order ${input.id}`); #{ total: input.price * input.qty, at: context.now }",
            "input": {"id": "A-1", "price": 2.5, "qty": 4}
        }))
        .await
        .unwrap();
    assert_eq!(out["total"], json!(10.0));
    assert_eq!(out["at"], json!("2026-01-01T00:00:00Z"));
    assert_eq!(out["_logs"], json!(["order A-1"]));
}

#[tokio::test]
async fn failures_are_coded_and_not_retried() {
    let c = ScriptConnector::new();
    let err = c
        .execute(&script_ctx(), "run", json!({"code": "throw \"bad\""}))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "script.error");
    assert!(!err.retryable());
    let no_runner = c
        .execute(
            &ctx(
                Arc::new(ReqwestSender::new(SsrfPolicy::public_only()).unwrap()),
                None,
            ),
            "run",
            json!({"code": "1"}),
        )
        .await
        .unwrap_err();
    assert!(no_runner.to_string().contains("not available"));
    let bad_input = c
        .execute(&script_ctx(), "run", json!({"code": "1", "input": [1]}))
        .await
        .unwrap_err();
    assert_eq!(bad_input.code(), "app.invalid_input");
}
