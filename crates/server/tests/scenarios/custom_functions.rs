//! Custom functions (`core.script`): run in a flow, syntax errors block publishing with a line
//! number, `throw` fails the run without retries, and "Test step" runs the script once.

use serde_json::{Value, json};

use crate::harness::TestApp;

const SUM: &str = "let total = 0.0;\nfor item in input.items { total += item.price * item.qty.to_float(); }\nprint(`items: ${input.items.len()}`);\n#{ total: total, is_big: total > 100.0 }";

fn flow(code: &str) -> Value {
    json!({"trigger": {"type": "webhook"}, "steps": [
        {"key": "fn", "type": "action", "connector": "core.script", "action": "run",
            "input": {"code": code, "input": {"items": "{{ trigger.body.items }}"}}},
        {"key": "big", "type": "decision", "branches": [
            {"key": "yes", "condition": {"left": "{{ steps.fn.is_big }}", "op": "is_true"}, "steps": [
                {"key": "done", "type": "stop", "status": "succeeded", "message": "big order"}]}
        ], "otherwise": []}
    ]})
}

async fn run(app: &TestApp, flow_id: &str, payload: Value) -> Value {
    let r = app
        .ok(
            "POST",
            &format!("/api/v1/flows/{flow_id}/run"),
            Some(json!({"payload": payload})),
        )
        .await;
    app.drain().await;
    app.ok(
        "GET",
        &format!("/api/v1/executions/{}", r["execution_id"].as_str().unwrap()),
        None,
    )
    .await
}

fn step<'a>(detail: &'a Value, key: &str) -> &'a Value {
    detail["steps"]
        .as_array()
        .unwrap()
        .iter()
        .rev()
        .find(|s| s["step_key"] == key)
        .unwrap()
}

#[tokio::test]
async fn a_custom_function_computes_values_later_steps_use() {
    let app = TestApp::spawn().await;
    let f = app.live_flow("Order total", flow(SUM)).await;
    let detail = run(
        &app,
        f["id"].as_str().unwrap(),
        json!({"body": {"items": [
            {"price": 40.0, "qty": 2}, {"price": 25.5, "qty": 1}
        ]}}),
    )
    .await;
    assert_eq!(detail["status"], "succeeded", "{detail}");
    let out = &step(&detail, "fn")["output"];
    assert_eq!(out["total"], json!(105.5));
    assert_eq!(out["_logs"], json!(["items: 2"]));
    assert_eq!(step(&detail, "big")["output"]["branch"], "yes");
    app.cleanup().await;
}

#[tokio::test]
async fn syntax_errors_block_publishing_with_the_line_number() {
    let app = TestApp::spawn().await;
    let created = app
        .ok(
            "POST",
            "/api/v1/flows",
            Some(json!({"name": "Broken", "definition": flow("let x = 1;\nlet y = ;")})),
        )
        .await;
    let id = created["id"].as_str().unwrap();
    let issues = app
        .ok("POST", &format!("/api/v1/flows/{id}/validate"), None)
        .await;
    let issue = issues["issues"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["path"] == "/steps/0/input/code")
        .cloned();
    let issue = issue.unwrap_or_else(|| panic!("no issue on the code: {issues}"));
    assert_eq!(issue["severity"], "error");
    assert!(
        issue["message"].as_str().unwrap().starts_with("line 2"),
        "{issue}"
    );
    let (status, body) = app
        .req("POST", &format!("/api/v1/flows/{id}/publish"), None)
        .await;
    assert_eq!(status.as_u16(), 422, "{body}");
    app.cleanup().await;
}

#[tokio::test]
async fn a_thrown_error_fails_the_run_once_without_retries() {
    let app = TestApp::spawn().await;
    let f = app
        .live_flow(
            "Guard",
            flow("if input.items.len() == 0 { throw \"no items\"; }\n#{ is_big: false }"),
        )
        .await;
    let detail = run(
        &app,
        f["id"].as_str().unwrap(),
        json!({"body": {"items": []}}),
    )
    .await;
    assert_eq!(detail["status"], "failed", "{detail}");
    assert_eq!(detail["error_code"], "script.error");
    let attempts = detail["steps"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["step_key"] == "fn")
        .count();
    assert_eq!(attempts, 1, "configuration errors are not retried");
    app.cleanup().await;
}

#[tokio::test]
async fn test_step_runs_the_script_once_with_sample_data() {
    let app = TestApp::spawn().await;
    let created = app
        .ok(
            "POST",
            "/api/v1/flows",
            Some(json!({"name": "Draft", "definition": flow(SUM)})),
        )
        .await;
    let id = created["id"].as_str().unwrap();
    let tested = app
        .ok(
            "POST",
            &format!("/api/v1/flows/{id}/steps/fn/test"),
            Some(json!({"context": {
                "trigger": {"body": {"items": [{"price": 10.0, "qty": 3}]}}, "steps": {}, "vars": {}
            }})),
        )
        .await;
    assert_eq!(tested["output"]["total"], json!(30.0), "{tested}");
    assert_eq!(tested["input"]["input"]["items"][0]["qty"], json!(3));
    app.cleanup().await;
}
