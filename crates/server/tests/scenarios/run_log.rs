//! The detailed run log (`GET /executions/{id}/log`) through the real runner: every HTTP call of every attempt
//! (without secrets), the scheduled retry, a request held back by a connection's limits, and the run's end.

use chrono::Duration;
use serde_json::{Value, json};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path, path_regex},
};

use crate::harness::TestApp;

async fn run(app: &TestApp, flow: &str) -> String {
    let r = app
        .ok(
            "POST",
            &format!("/api/v1/flows/{flow}/run"),
            Some(json!({"payload": {"id": "01JOB"}})),
        )
        .await;
    r["execution_id"].as_str().unwrap().to_owned()
}

fn of_kind<'a>(log: &'a Value, kind: &str) -> Vec<&'a Value> {
    log["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == kind)
        .collect()
}

#[tokio::test]
async fn the_log_shows_each_call_of_each_attempt_and_the_retry_without_secrets() {
    let app = TestApp::spawn().await;
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(503).set_body_string("busy, try later"))
        .up_to_n_times(1)
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
        .mount(&mock)
        .await;
    let url = format!("{}/items?page=1&token=s3cret", mock.uri());
    let flow = app
        .live_flow(
            "Fetch",
            json!({"trigger": {"type": "manual"}, "steps": [
                {"key": "call", "type": "action", "connector": "core.http", "action": "request",
                    "input": {"method": "GET", "url": url}}
            ]}),
        )
        .await;
    let exe = run(&app, flow["id"].as_str().unwrap()).await;
    app.drain().await;
    app.clock.advance(Duration::minutes(2));
    app.drain().await;

    let log = app
        .ok("GET", &format!("/api/v1/executions/{exe}/log"), None)
        .await;
    assert_eq!(log["truncated"], false);
    let text = log.to_string();
    assert!(!text.contains("s3cret"), "no secret query values: {log}");
    let calls = of_kind(&log, "http");
    assert_eq!(calls.len(), 2, "{log}");
    assert_eq!(calls[0]["attempt"], 1);
    assert_eq!(calls[0]["details"]["status"], 503);
    assert_eq!(calls[0]["details"]["response_excerpt"], "busy, try later");
    assert_eq!(calls[0]["level"], "warn");
    assert!(
        calls[0]["details"]["url"]
            .as_str()
            .unwrap()
            .ends_with("/items?page=1&token=%E2%80%A6"),
        "{log}"
    );
    assert_eq!(calls[1]["attempt"], 2);
    assert_eq!(calls[1]["details"]["status"], 200);
    assert_eq!(calls[1]["details"]["response_excerpt"], Value::Null);
    let retries = of_kind(&log, "retry");
    assert_eq!(retries.len(), 1, "{log}");
    assert_eq!(retries[0]["details"]["next_attempt"], 2);
    let last = log["entries"].as_array().unwrap().last().unwrap();
    assert_eq!(last["message"], "Run succeeded in 2 min 0 s");
    // Unknown runs are not found.
    let (status, _) = app
        .req(
            "GET",
            "/api/v1/executions/exe_01900000000070008000000000000099/log",
            None,
        )
        .await;
    assert_eq!(status.as_u16(), 404);
    app.cleanup().await;
}

#[tokio::test]
async fn a_request_held_back_by_a_limit_is_in_the_log_but_is_not_a_call() {
    let app = TestApp::spawn().await;
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/modules"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/api/jobs/.+$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"api_id": "01JOB"})))
        .mount(&server)
        .await;
    let cxn = app
        .ok(
            "POST",
            "/api/v1/connections",
            Some(json!({"connector": "appblocks", "name": "FSM",
                "fields": {"base_url": server.uri(), "org_id": "1", "api_token": "apb_test"}})),
        )
        .await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    app.ok(
        "PUT",
        &format!("/api/v1/connections/{cxn}/limits"),
        Some(json!({"requests_per_minute": 1})),
    )
    .await;
    let flow = app
        .live_flow("Get job", json!({"trigger": {"type": "manual"}, "steps": [
            {"key": "job", "type": "action", "connector": "appblocks", "action": "get_record", "connection_id": cxn,
                "input": {"module": "jobs", "record_id": "{{ trigger.id }}"}}
        ]}))
        .await;
    let flow = flow["id"].as_str().unwrap();
    // The connection test used this minute's one request.
    let exe = run(&app, flow).await;
    app.drain().await;
    let log = app
        .ok("GET", &format!("/api/v1/executions/{exe}/log"), None)
        .await;
    let held = of_kind(&log, "limit");
    assert_eq!(held.len(), 1, "{log}");
    assert_eq!(held[0]["step_key"], "job");
    assert_eq!(held[0]["details"]["reason"], "1 requests per minute");
    assert!(of_kind(&log, "http").is_empty(), "nothing was sent: {log}");
    assert_eq!(
        log["entries"].as_array().unwrap().last().unwrap()["message"],
        "Waiting"
    );

    app.clock.advance(Duration::seconds(67));
    app.drain().await;
    let log = app
        .ok("GET", &format!("/api/v1/executions/{exe}/log"), None)
        .await;
    let calls = of_kind(&log, "http");
    assert_eq!(calls.len(), 1, "{log}");
    assert_eq!(
        calls[0]["attempt"], 1,
        "the held-back request was not an attempt"
    );
    assert!(
        calls[0]["message"]
            .as_str()
            .unwrap()
            .starts_with("GET http://127.0.0.1"),
        "{log}"
    );
    app.cleanup().await;
}
