//! Per-connection limits: a per-minute bucket and a daily budget, set through the API, hold back
//! requests to the app (the step waits and retries); usage shows the warning and critical levels.

use chrono::Duration;
use serde_json::{Value, json};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path, path_regex},
};

use crate::harness::TestApp;

async fn appblocks(app: &TestApp, server: &MockServer) -> String {
    Mock::given(method("GET"))
        .and(path("/api/modules"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/api/jobs/.+$"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"api_id": "01JOB", "status": "Open"})),
        )
        .mount(server)
        .await;
    let c = app
        .ok(
            "POST",
            "/api/v1/connections",
            Some(json!({"connector": "appblocks", "name": "FSM",
            "fields": {"base_url": server.uri(), "org_id": "1", "api_token": "apb_test"}})),
        )
        .await;
    c["id"].as_str().unwrap().to_owned()
}

async fn run(app: &TestApp, flow: &str) -> Value {
    let r = app
        .ok(
            "POST",
            &format!("/api/v1/flows/{flow}/run"),
            Some(json!({"payload": {"id": "01JOB"}})),
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

#[tokio::test]
async fn limits_hold_back_requests_and_usage_reports_the_budget() {
    let app = TestApp::spawn().await;
    let server = MockServer::start().await;
    let cxn = appblocks(&app, &server).await; // the connection test made 1 request
    let defaults = app
        .ok("GET", &format!("/api/v1/connections/{cxn}/limits"), None)
        .await;
    assert_eq!(defaults["defaults"]["daily_requests"], 10_000, "{defaults}");
    let set = app
        .ok(
            "PUT",
            &format!("/api/v1/connections/{cxn}/limits"),
            Some(json!({"requests_per_minute": 2, "daily_requests": 5})),
        )
        .await;
    assert_eq!(
        set["effective"],
        json!({"requests_per_minute": 2, "max_concurrent": 10, "daily_requests": 5})
    );
    let flow = app
        .live_flow("Get job", json!({"trigger": {"type": "manual"}, "steps": [
            {"key": "job", "type": "action", "connector": "appblocks", "action": "get_record", "connection_id": cxn,
                "input": {"module": "jobs", "record_id": "{{ trigger.id }}"}}
        ]}))
        .await;
    let flow = flow["id"].as_str().unwrap();

    assert_eq!(run(&app, flow).await["status"], "succeeded");
    assert_eq!(run(&app, flow).await["status"], "succeeded");
    // 2 per minute: the third request waits for the bucket instead of reaching the app.
    let held = run(&app, flow).await;
    assert_eq!(held["status"], "waiting", "{held}");
    app.clock.advance(Duration::seconds(61));
    app.drain().await;
    let done = app
        .ok(
            "GET",
            &format!("/api/v1/executions/{}", held["id"].as_str().unwrap()),
            None,
        )
        .await;
    assert_eq!(done["status"], "succeeded", "{done}");

    let usage = app
        .ok("GET", &format!("/api/v1/connections/{cxn}/limits"), None)
        .await;
    assert_eq!(usage["usage"]["requests"], 4, "{usage}");
    assert_eq!(usage["usage"]["level"], "warning", "80% of 5");

    app.clock.advance(Duration::seconds(61));
    assert_eq!(run(&app, flow).await["status"], "succeeded"); // the 5th request of the day
    app.clock.advance(Duration::seconds(61));
    let over = run(&app, flow).await;
    assert_eq!(
        over["status"], "waiting",
        "the daily budget is used up: {over}"
    );
    let list = app.ok("GET", "/api/v1/connections/usage", None).await;
    let row = list
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["connection_id"] == cxn.as_str())
        .unwrap()
        .clone();
    assert_eq!(row["level"], "critical", "{row}");
    assert_eq!(row["refused"], 1);

    // Raising the budget takes effect at once (the cached limits are dropped).
    app.ok(
        "PUT",
        &format!("/api/v1/connections/{cxn}/limits"),
        Some(json!({"requests_per_minute": 2, "daily_requests": 0})),
    )
    .await;
    let unlimited = app
        .ok("GET", &format!("/api/v1/connections/{cxn}/limits"), None)
        .await;
    assert_eq!(unlimited["effective"]["daily_requests"], Value::Null);
    app.cleanup().await;
}

#[tokio::test]
async fn limits_reject_absurd_values_and_unknown_connections() {
    let app = TestApp::spawn().await;
    let server = MockServer::start().await;
    let cxn = appblocks(&app, &server).await;
    let (status, _) = app
        .req(
            "PUT",
            &format!("/api/v1/connections/{cxn}/limits"),
            Some(json!({"requests_per_minute": 2_000_000_000u64})),
        )
        .await;
    assert_eq!(status.as_u16(), 422);
    let (status, _) = app
        .req(
            "GET",
            "/api/v1/connections/cxn_01900000000070008000000000000099/limits",
            None,
        )
        .await;
    assert_eq!(status.as_u16(), 404);
    app.cleanup().await;
}

#[tokio::test]
async fn a_held_back_request_is_not_an_attempt_and_is_never_skipped() {
    let app = TestApp::spawn().await;
    let server = MockServer::start().await;
    let cxn = appblocks(&app, &server).await;
    app.ok(
        "PUT",
        &format!("/api/v1/connections/{cxn}/limits"),
        Some(json!({"requests_per_minute": 1})),
    )
    .await;
    // One attempt only, and "continue" on error: a throttled request must neither fail nor skip.
    let flow = app
        .live_flow("Strict", json!({"trigger": {"type": "manual"}, "steps": [
            {"key": "job", "type": "action", "connector": "appblocks", "action": "get_record", "connection_id": cxn,
                "input": {"module": "jobs", "record_id": "{{ trigger.id }}"},
                "retry": {"max_attempts": 1}, "on_error": "continue"}
        ]}))
        .await;
    let flow = flow["id"].as_str().unwrap();
    assert_eq!(run(&app, flow).await["status"], "succeeded");
    let held = run(&app, flow).await;
    assert_eq!(
        held["status"], "waiting",
        "held back, not failed or skipped: {held}"
    );
    assert_eq!(
        held["steps"].as_array().map(Vec::len),
        Some(0),
        "a held-back request leaves no history row: {held}"
    );
    // The park time is the limiter's hint (at most 60 s) plus a spread of at most 10% of it.
    app.clock.advance(Duration::seconds(67));
    app.drain().await;
    let done = app
        .ok(
            "GET",
            &format!("/api/v1/executions/{}", held["id"].as_str().unwrap()),
            None,
        )
        .await;
    assert_eq!(done["status"], "succeeded", "{done}");
    let jobs: Vec<&Value> = done["steps"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["step_key"] == "job")
        .collect();
    assert_eq!(jobs.len(), 1, "one attempt in the history: {done}");
    let job = jobs[0];
    assert_eq!(
        job["output"]["record"]["api_id"], "01JOB",
        "the request was sent once the limit allowed: {done}"
    );
    app.cleanup().await;
}
