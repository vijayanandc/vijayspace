//! Polling triggers start where the flow was switched on: the first poll runs at once and records where changes
//! start, so a record changed during the first interval is not missed (live FSM test, 2026-11-18).

use chrono::Duration;
use serde_json::json;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path, query_param},
};

use crate::harness::TestApp;

#[tokio::test]
async fn a_record_changed_right_after_the_flow_is_switched_on_starts_a_run() {
    let app = TestApp::spawn().await;
    let fsm = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/modules"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&fsm)
        .await;
    // Changes since the cursor: one contact, edited just after the flow was switched on.
    Mock::given(method("GET"))
        .and(path("/api/contacts"))
        .and(query_param("sort_by", "modified_time"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"api_id": "01CONTACT", "name": "Pat", "modified_time": "2026-01-01T00:00:10Z"}
        ])))
        .mount(&fsm)
        .await;
    let cxn = app
        .ok(
            "POST",
            "/api/v1/connections",
            Some(json!({"connector": "appblocks", "name": "FSM",
                "fields": {"base_url": fsm.uri(), "org_id": "1", "api_token": "apb_test"}})),
        )
        .await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let flow = app
        .live_flow("Contacts", json!({
            "trigger": {"type": "app", "connector": "appblocks", "trigger": "record_changed", "connection_id": cxn,
                "input": {"module": "contacts"}, "poll_interval_secs": 60, "debounce_secs": 0},
            "steps": [{"key": "say", "type": "action", "connector": "core.formatter", "action": "text_transform",
                "input": {"text": "{{ trigger.record.name }}", "operation": "upper"}}]
        }))
        .await;
    let flow_id = flow["id"].as_str().unwrap();
    // Switched on: the first poll runs now and only records the start (no request, no run).
    app.drain().await;
    assert!(app.executions(flow_id).await.is_empty());
    let list_calls = || async {
        fsm.received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.url.path() == "/api/contacts")
            .count()
    };
    assert_eq!(
        list_calls().await,
        0,
        "the first poll only records where to start"
    );
    // One interval later the edit made in between starts a run.
    app.clock.advance(Duration::seconds(61));
    app.drain().await;
    assert_eq!(list_calls().await, 1);
    let runs = app.executions(flow_id).await;
    assert_eq!(runs.len(), 1, "{runs:?}");
    assert_eq!(runs[0]["status"], "succeeded");
    app.cleanup().await;
}
