//! Flow and execution lifecycle: drafts, validation, publish, retry, re-run, cancel, test runs.

use chrono::Duration;
use serde_json::json;
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};

use crate::harness::TestApp;

#[tokio::test]
async fn drafts_validation_and_publish_rules() {
    let app = TestApp::spawn().await;
    let flow = app
        .ok("POST", "/api/v1/flows", Some(json!({"name": "F"})))
        .await;
    let id = flow["id"].as_str().unwrap();
    let bad = json!({"trigger": {"type": "manual"}, "steps": [{"key": "x", "type": "action", "connector": "core.http", "action": "request", "input": {}}]});
    let saved = app
        .ok(
            "PUT",
            &format!("/api/v1/flows/{id}/draft"),
            Some(json!({"definition": bad, "revision": 1})),
        )
        .await;
    assert_eq!(saved["revision"], 2);
    assert_eq!(saved["issues"][0]["code"], "field.required");
    let (status, conflict) = app
        .req(
            "PUT",
            &format!("/api/v1/flows/{id}/draft"),
            Some(json!({"definition": bad, "revision": 1})),
        )
        .await;
    assert_eq!(
        (status.as_u16(), conflict["code"].as_str()),
        (409, Some("flow.revision_conflict"))
    );
    let (status, invalid) = app
        .req("POST", &format!("/api/v1/flows/{id}/publish"), None)
        .await;
    assert_eq!(
        (status.as_u16(), invalid["code"].as_str()),
        (422, Some("flow.invalid"))
    );
    assert_eq!(invalid["issues"][0]["path"], "/steps/0/input/url");
    let (status, _) = app
        .req("POST", &format!("/api/v1/flows/{id}/enable"), None)
        .await;
    assert_eq!(status, 409, "cannot enable an unpublished flow");
    let (status, _) = app
        .req(
            "GET",
            "/api/v1/flows/flw_00000000000070008000000000000099",
            None,
        )
        .await;
    assert_eq!(status, 404);
    app.cleanup().await;
}

#[tokio::test]
async fn failed_executions_can_be_retried_rerun_and_waiting_ones_cancelled() {
    let app = TestApp::spawn().await;
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(400).set_body_string("bad"))
        .up_to_n_times(1)
        .mount(&mock)
        .await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": 1})))
        .mount(&mock)
        .await;
    let def = json!({"trigger": {"type": "manual"}, "steps": [
        {"key": "call", "type": "action", "connector": "core.http", "action": "request", "input": {"method": "POST", "url": mock.uri(), "body": {"n": "{{ trigger.n }}"}}},
        {"key": "pause", "type": "delay", "seconds": 600}
    ]});
    let flow = app.live_flow("Retry me", def).await;
    let flow_id = flow["id"].as_str().unwrap();
    let r = app
        .ok(
            "POST",
            &format!("/api/v1/flows/{flow_id}/run"),
            Some(json!({"payload": {"n": 1}})),
        )
        .await;
    let exe = r["execution_id"].as_str().unwrap().to_owned();
    app.drain().await;
    let failed = app
        .ok("GET", &format!("/api/v1/executions/{exe}"), None)
        .await;
    assert_eq!(
        failed["status"], "failed",
        "a 400 is a configuration error: no automatic retry"
    );
    assert_eq!(failed["error_code"], "app.invalid_input");
    assert_eq!(failed["failed_step_key"], "call");

    app.ok("POST", &format!("/api/v1/executions/{exe}/retry"), None)
        .await;
    app.drain().await;
    let resumed = app
        .ok("GET", &format!("/api/v1/executions/{exe}"), None)
        .await;
    assert_eq!(
        resumed["status"], "waiting",
        "retried from the failed step, now in the delay"
    );

    let cancelled = app
        .ok("POST", &format!("/api/v1/executions/{exe}/cancel"), None)
        .await;
    assert_eq!(cancelled["id"], exe.as_str());
    app.drain().await;
    let after = app
        .ok("GET", &format!("/api/v1/executions/{exe}"), None)
        .await;
    assert_eq!(after["status"], "cancelled");

    let rerun = app
        .ok("POST", &format!("/api/v1/executions/{exe}/rerun"), None)
        .await;
    app.drain().await;
    app.clock.advance(Duration::minutes(11));
    app.drain().await;
    let rr = app
        .ok(
            "GET",
            &format!(
                "/api/v1/executions/{}",
                rerun["execution_id"].as_str().unwrap()
            ),
            None,
        )
        .await;
    assert_eq!(rr["status"], "succeeded");
    assert_eq!(rr["mode"], "rerun");
    assert_eq!(rr["trigger_data"], json!({"n": 1}));
    app.cleanup().await;
}

#[tokio::test]
async fn test_runs_use_the_draft_and_step_tests_render_inputs() {
    let app = TestApp::spawn().await;
    let def = json!({"trigger": {"type": "webhook"}, "steps": [
        {"key": "name", "type": "action", "connector": "core.formatter", "action": "split_name", "input": {"full_name": "{{ trigger.body.name }}"}},
        {"key": "hi", "type": "action", "connector": "core.formatter", "action": "text_transform", "input": {"text": "hi {{ steps.name.first_name }}", "operation": "upper"}}
    ]});
    let flow = app
        .ok(
            "POST",
            "/api/v1/flows",
            Some(json!({"name": "T", "definition": def})),
        )
        .await;
    let id = flow["id"].as_str().unwrap();
    let step = app
        .ok(
            "POST",
            &format!("/api/v1/flows/{id}/steps/hi/test"),
            Some(json!({"context": {"steps": {"name": {"first_name": "Ann"}}}})),
        )
        .await;
    assert_eq!(step["input"]["text"], "hi Ann");
    assert_eq!(step["output"]["result"], "HI ANN");
    let r = app
        .ok(
            "POST",
            &format!("/api/v1/flows/{id}/test"),
            Some(json!({"payload": {"body": {"name": "Ann Lee"}}})),
        )
        .await;
    app.drain().await;
    let exe = app
        .ok(
            "GET",
            &format!("/api/v1/executions/{}", r["execution_id"].as_str().unwrap()),
            None,
        )
        .await;
    assert_eq!(exe["status"], "succeeded");
    assert_eq!(exe["mode"], "test");
    let usage = app.ok("GET", "/api/v1/usage", None).await;
    assert_eq!(usage["tasks_used"], 2);
    let list = app.ok("GET", "/api/v1/flows", None).await;
    assert_eq!(list[0]["last_execution"]["status"], "succeeded");
    assert_eq!(list[0]["has_unpublished_changes"], true);
    app.cleanup().await;
}
