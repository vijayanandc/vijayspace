//! Run history retention: history lives in day partitions and is kept 30 days at most (older partitions are
//! dropped); an org that keeps less loses its older finished runs sooner; a run past the cutoff can be re-run
//! but not retried.

use chrono::Duration;
use serde_json::{Value, json};
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};

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

/// Day partitions of run history in this test's database.
async fn partitions(app: &TestApp, table: &str) -> Vec<String> {
    let pool = weft_db::connect(&app.database_url(), 1).await.unwrap();
    let names: Vec<String> = sqlx::query_scalar(
        "SELECT CAST(partition_name AS CHAR) FROM information_schema.partitions \
         WHERE table_schema = DATABASE() AND table_name = ? AND partition_name IS NOT NULL",
    )
    .bind(table)
    .fetch_all(&pool)
    .await
    .unwrap();
    names
}

#[tokio::test]
async fn run_history_is_kept_30_days_in_day_partitions_and_then_dropped() {
    let app = TestApp::spawn().await;
    let quick = app
        .live_flow(
            "Quick",
            json!({"trigger": {"type": "webhook"}, "steps": [{"key": "s", "type": "stop"}]}),
        )
        .await;
    let old = webhook_run(&app, &quick).await;
    let draft = app.ok("POST", "/api/v1/flows", Some(json!({"name": "Draft", "definition": {"trigger": {"type": "manual"}, "steps": [{"key": "s", "type": "stop"}]}}))).await;
    app.ok(
        "POST",
        &format!("/api/v1/flows/{}/test", draft["id"].as_str().unwrap()),
        Some(json!({"payload": {}})),
    )
    .await;
    app.drain().await;
    let first_day = app.deps.clock.now().date_naive();

    app.clock.advance(Duration::days(29));
    let recent = webhook_run(&app, &quick).await;
    let early = weft_app::retention::sweep(&app.deps).await.unwrap();
    assert_eq!(early.partitions.expired_runs, 0, "{early:?}");
    assert!(exists(&app, &old).await, "29 days old: kept");
    let today = app.deps.clock.now().date_naive();
    for table in ["executions", "step_runs", "trigger_events"] {
        let names = partitions(&app, table).await;
        let ahead = weft_db::partitions::partition_name(today + Duration::days(7));
        assert!(
            names.contains(&ahead),
            "{table}: a week of day partitions ahead: {names:?}"
        );
        assert!(
            names.contains(&"pmax".to_owned()),
            "{table}: the catch-all stays"
        );
    }

    app.clock.advance(Duration::days(2)); // the first runs are now 31 days old
    let report = weft_app::retention::sweep(&app.deps).await.unwrap();
    assert!(
        report.partitions.dropped >= 3,
        "a day partition per table: {report:?}"
    );
    assert_eq!(report.partitions.rows_in_catch_all, 0, "{report:?}");
    assert_eq!(
        report.test_snapshots, 1,
        "the test run's snapshot: {report:?}"
    );
    assert!(!exists(&app, &old).await, "dropped with its day");
    assert!(exists(&app, &recent).await);
    let gone = weft_db::partitions::partition_name(first_day);
    for table in ["executions", "step_runs", "trigger_events"] {
        assert!(
            !partitions(&app, table).await.contains(&gone),
            "{table}: {gone} dropped"
        );
    }
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
    let untouched = app.ok("PATCH", "/api/v1/org", Some(json!({}))).await;
    assert_eq!(
        untouched["retention_days"], 7,
        "an absent field changes nothing"
    );

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

#[tokio::test]
async fn a_run_whose_history_is_being_removed_can_be_rerun_but_not_retried() {
    let app = TestApp::spawn().await;
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(400).set_body_string("bad"))
        .mount(&mock)
        .await;
    let flow = app
        .live_flow("Fails", json!({"trigger": {"type": "manual"}, "steps": [
            {"key": "call", "type": "action", "connector": "core.http", "action": "request", "input": {"method": "POST", "url": mock.uri()}}
        ]}))
        .await;
    let flow_id = flow["id"].as_str().unwrap();
    let mut runs = Vec::new();
    for _ in 0..2 {
        let started = app
            .ok(
                "POST",
                &format!("/api/v1/flows/{flow_id}/run"),
                Some(json!({"payload": {}})),
            )
            .await;
        runs.push(started["execution_id"].as_str().unwrap().to_owned());
    }
    app.drain().await;
    let (early, exe) = (&runs[0], &runs[1]);

    // Two hours before the retry cutoff (28 days after the run was queued) a retry is still allowed: it has
    // time to finish before its day is expired and dropped.
    app.clock.advance(Duration::days(27) + Duration::hours(22));
    app.ok("POST", &format!("/api/v1/executions/{early}/retry"), None)
        .await;

    // Past it, the retried run could be expired before it finishes, or lose its steps when its day is dropped.
    app.clock
        .advance(Duration::hours(2) + Duration::minutes(30));
    let (status, body) = app
        .req("POST", &format!("/api/v1/executions/{exe}/retry"), None)
        .await;
    assert_eq!(
        (status.as_u16(), body["code"].as_str()),
        (409, Some("execution.not_retryable")),
        "{body}"
    );
    let rerun = app
        .ok("POST", &format!("/api/v1/executions/{exe}/rerun"), None)
        .await;
    assert!(rerun["execution_id"].is_string(), "{rerun}");
    app.cleanup().await;
}

#[tokio::test]
async fn a_wait_that_would_outlast_the_history_fails_the_run_at_once() {
    let app = TestApp::spawn().await;
    let flow = app
        .live_flow(
            "Waits",
            json!({"trigger": {"type": "manual"}, "steps": [
                {"key": "wait", "type": "delay", "until": "{{ trigger.at }}"}
            ]}),
        )
        .await;
    let flow_id = flow["id"].as_str().unwrap();
    let now = weft_app::ports::Clock::now(&app.clock);
    let run = |days: i64| {
        let at = (now + Duration::days(days)).to_rfc3339();
        let app = &app;
        async move {
            let r = app
                .ok(
                    "POST",
                    &format!("/api/v1/flows/{flow_id}/run"),
                    Some(json!({"payload": {"at": at}})),
                )
                .await;
            r["execution_id"].as_str().unwrap().to_owned()
        }
    };
    let (short, long) = (run(20).await, run(29).await);
    app.drain().await;
    let short = app
        .ok("GET", &format!("/api/v1/executions/{short}"), None)
        .await;
    assert_eq!(short["status"], "waiting", "{short}");
    // History is kept 30 days from the day the run was queued: a 29-day wait would end with the run gone.
    let long = app
        .ok("GET", &format!("/api/v1/executions/{long}"), None)
        .await;
    assert_eq!(
        (long["status"].as_str(), long["error_code"].as_str()),
        (Some("failed"), Some("delay.too_long")),
        "{long}"
    );
    app.cleanup().await;
}
