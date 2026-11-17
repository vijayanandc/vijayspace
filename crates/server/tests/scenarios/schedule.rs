//! Schedule triggers: one execution per slot, nothing in between.

use chrono::Duration;
use serde_json::json;

use crate::harness::TestApp;

#[tokio::test]
async fn interval_schedule_fires_once_per_slot() {
    let app = TestApp::spawn().await;
    let def = json!({
        "trigger": {"type": "schedule", "schedule": {"kind": "interval", "minutes": 15}},
        "steps": [{"key": "fmt", "type": "action", "connector": "core.formatter", "action": "date_format", "input": {"date": "{{ trigger.scheduled_for }}", "format": "%H:%M"}}]
    });
    let flow = app.live_flow("Every 15 min", def).await;
    let flow_id = flow["id"].as_str().unwrap();
    assert!(flow["trigger"]["next_fire_at"].is_string());
    app.drain().await;
    assert!(
        app.executions(flow_id).await.is_empty(),
        "nothing is due yet"
    );
    app.clock.advance(Duration::minutes(15));
    app.drain().await;
    app.drain().await;
    let runs = app.executions(flow_id).await;
    assert_eq!(runs.len(), 1, "exactly one execution per slot");
    assert_eq!(runs[0]["status"], "succeeded");
    app.clock.advance(Duration::minutes(15));
    app.drain().await;
    assert_eq!(app.executions(flow_id).await.len(), 2);
    // Turning the flow off stops the schedule.
    app.ok("POST", &format!("/api/v1/flows/{flow_id}/disable"), None)
        .await;
    app.clock.advance(Duration::minutes(30));
    app.drain().await;
    assert_eq!(app.executions(flow_id).await.len(), 2);
    app.cleanup().await;
}
