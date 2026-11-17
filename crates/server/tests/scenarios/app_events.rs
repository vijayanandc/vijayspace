//! App events with debounce: a burst runs once with the newest state (a delete is never lost),
//! and the reconciliation poll does not re-run a version already delivered live.

use chrono::Duration;
use serde_json::{Value, json};

use crate::harness::TestApp;

async fn post_event(app: &TestApp, path: &str, body: Value) -> Value {
    let (status, json) = app.req("POST", path, Some(body)).await;
    assert!(status.is_success(), "{status}: {json}");
    json
}

#[tokio::test]
async fn debounced_app_events_keep_the_newest_state_and_reconciliation_does_not_rerun() {
    let app = TestApp::spawn().await;
    let src = app.connection("memsrc", "Source").await;
    let flow = app
        .live_flow("Records", json!({
            "trigger": {"type": "app", "connector": "memsrc", "trigger": "record_event", "connection_id": src,
                "input": {}, "debounce_secs": 30, "poll_interval_secs": 300},
            "steps": [{"key": "op", "type": "action", "connector": "core.formatter", "action": "text_transform",
                "input": {"text": "{{ trigger.operation }}", "operation": "upper"}}]
        }))
        .await;
    let flow_id = flow["id"].as_str().unwrap();
    let url = flow["trigger"]["app_event_url"].as_str().unwrap();
    let path = &url[url.find("/hooks/").unwrap()..];

    // An edit then a delete of the same record within one 30 s window: one run, with the delete.
    let first = post_event(
        &app,
        path,
        json!({"id": "A1", "modified": "t1", "record": {"name": "Acme"}}),
    )
    .await;
    assert_eq!(first["executions"].as_array().unwrap().len(), 1, "{first}");
    let second = post_event(
        &app,
        path,
        json!({"id": "A1", "deleted": true, "record": null}),
    )
    .await;
    assert_eq!(
        second["executions"].as_array().unwrap().len(),
        0,
        "coalesced into the queued run: {second}"
    );
    app.clock.advance(Duration::seconds(31));
    app.drain().await;
    let runs = app.executions(flow_id).await;
    assert_eq!(runs.len(), 1);
    let detail = app
        .ok(
            "GET",
            &format!("/api/v1/executions/{}", runs[0]["id"].as_str().unwrap()),
            None,
        )
        .await;
    assert_eq!(detail["status"], "succeeded", "{detail}");
    assert_eq!(
        detail["trigger_data"]["operation"], "delete",
        "the newest event wins"
    );

    // The hourly reconciliation poll returns the version already delivered live: no second run.
    app.src
        .pending_poll
        .lock()
        .unwrap()
        .push(json!({"id": "A1", "modified": "t1", "record": {"name": "Acme"}}));
    app.clock.advance(Duration::seconds(301));
    app.drain().await;
    assert_eq!(
        app.executions(flow_id).await.len(),
        1,
        "a re-delivered version must not run again"
    );

    // A genuinely new version runs.
    post_event(
        &app,
        path,
        json!({"id": "A1", "modified": "t2", "record": {"name": "Acme Ltd"}}),
    )
    .await;
    app.clock.advance(Duration::seconds(31));
    app.drain().await;
    assert_eq!(app.executions(flow_id).await.len(), 2);
    app.cleanup().await;
}
