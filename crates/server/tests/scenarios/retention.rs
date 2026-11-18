//! Run history retention: finished runs older than the org's retention are deleted with their
//! step runs, trigger events and test snapshots; runs still waiting are never touched; the org
//! setting overrides the server default.

use chrono::Duration;
use serde_json::{Value, json};

use crate::harness::TestApp;

async fn webhook_run(app: &TestApp, flow: &Value) -> String {
    let url = flow["trigger"]["webhook_url"].as_str().unwrap();
    let path = &url[url.find("/hooks/").unwrap()..];
    let (status, body) = app.req("POST", path, Some(json!({"n": 1}))).await;
    assert!(status.is_success(), "{body}");
    app.drain().await;
    body["executions"][0].as_str().unwrap().to_owned()
}

async fn exists(app: &TestApp, exe: &str) -> bool {
    app.req("GET", &format!("/api/v1/executions/{exe}"), None)
        .await
        .0
        .is_success()
}

#[tokio::test]
async fn finished_runs_past_retention_are_deleted_and_waiting_ones_are_kept() {
    let app = TestApp::spawn().await;
    let quick = app
        .live_flow(
            "Quick",
            json!({"trigger": {"type": "webhook"}, "steps": [{"key": "s", "type": "stop"}]}),
        )
        .await;
    let slow = app
        .live_flow("Slow", json!({"trigger": {"type": "webhook"}, "steps": [{"key": "wait", "type": "delay", "seconds": 2_592_000}]}))
        .await;
    let old = webhook_run(&app, &quick).await;
    let waiting = webhook_run(&app, &slow).await;
    let draft = app.ok("POST", "/api/v1/flows", Some(json!({"name": "Draft", "definition": {"trigger": {"type": "manual"}, "steps": [{"key": "s", "type": "stop"}]}}))).await;
    app.ok(
        "POST",
        &format!("/api/v1/flows/{}/test", draft["id"].as_str().unwrap()),
        Some(json!({"payload": {}})),
    )
    .await;
    app.drain().await;

    app.clock.advance(Duration::days(29));
    let recent = webhook_run(&app, &quick).await;
    let early = weft_app::retention::sweep(&app.deps).await.unwrap();
    assert_eq!(
        early.executions, 0,
        "nothing is older than 30 days yet: {early:?}"
    );

    app.clock.advance(Duration::days(2)); // the first runs are now 31 days old
    let report = weft_app::retention::sweep(&app.deps).await.unwrap();
    assert_eq!(
        report.executions, 2,
        "the old webhook run and the test run: {report:?}"
    );
    assert!(report.step_runs >= 2, "{report:?}");
    assert!(report.trigger_events >= 1, "{report:?}");
    assert_eq!(report.test_snapshots, 1, "{report:?}");
    assert!(!exists(&app, &old).await);
    assert!(
        exists(&app, &waiting).await,
        "a waiting run is never deleted"
    );
    assert!(exists(&app, &recent).await);
    app.cleanup().await;
}

#[tokio::test]
async fn the_org_setting_overrides_the_default_and_is_validated() {
    let app = TestApp::spawn().await;
    let org = app.ok("GET", "/api/v1/org", None).await;
    assert_eq!(org["retention_days_effective"], 30);
    assert_eq!(org["retention_days_default"], 30);
    let set = app
        .ok("PATCH", "/api/v1/org", Some(json!({"retention_days": 7})))
        .await;
    assert_eq!(set["retention_days"], 7);
    assert_eq!(set["retention_days_effective"], 7);
    let (status, _) = app
        .req("PATCH", "/api/v1/org", Some(json!({"retention_days": 0})))
        .await;
    assert_eq!(status.as_u16(), 422);

    let quick = app
        .live_flow(
            "Quick",
            json!({"trigger": {"type": "webhook"}, "steps": [{"key": "s", "type": "stop"}]}),
        )
        .await;
    let run = webhook_run(&app, &quick).await;
    app.clock.advance(Duration::days(8));
    weft_app::retention::sweep(&app.deps).await.unwrap();
    assert!(!exists(&app, &run).await, "7-day retention");

    let reset = app
        .ok(
            "PATCH",
            "/api/v1/org",
            Some(json!({"retention_days": null})),
        )
        .await;
    assert_eq!(reset["retention_days"], Value::Null);
    assert_eq!(reset["retention_days_effective"], 30);
    app.cleanup().await;
}
