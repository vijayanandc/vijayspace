//! The MVP "Order alert" demo (doc 01 §3): split webhook, decision, retry, email, delay.

use chrono::Duration;
use serde_json::json;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

use crate::harness::TestApp;

#[tokio::test]
async fn order_alert_demo() {
    let app = TestApp::spawn().await;
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/notify"))
        .respond_with(ResponseTemplate::new(500))
        .up_to_n_times(1)
        .mount(&mock)
        .await;
    Mock::given(method("POST"))
        .and(path("/notify"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
        .mount(&mock)
        .await;
    Mock::given(method("POST"))
        .and(path("/review"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"id": "r1"})))
        .mount(&mock)
        .await;
    let def = json!({
        "trigger": {"type": "webhook", "split_array": true},
        "steps": [
            {"key": "is_big", "type": "decision", "branches": [
                {"key": "big", "condition": {"left": "{{ trigger.body.amount }}", "op": "gt", "right": 100},
                 "steps": [{"key": "notify", "type": "action", "connector": "core.http", "action": "request",
                            "input": {"method": "POST", "url": format!("{}/notify", mock.uri()), "body": {"email": "{{ trigger.body.email }}", "amount": "{{ trigger.body.amount }}"}}}]}
            ], "otherwise": [
                {"key": "pretty", "type": "action", "connector": "core.formatter", "action": "text_transform", "input": {"text": "{{ trigger.body.name }}", "operation": "title"}},
                {"key": "thank", "type": "action", "connector": "core.email", "action": "send",
                 "input": {"to": "{{ trigger.body.email }}", "subject": "Thanks {{ steps.pretty.result }}", "body": "We got your order."}}
            ]},
            {"key": "wait", "type": "delay", "seconds": 86400},
            {"key": "review", "type": "action", "connector": "core.http", "action": "request",
             "input": {"method": "POST", "url": format!("{}/review", mock.uri()), "body": {"email": "{{ trigger.body.email }}"}}}
        ]
    });
    let flow = app.live_flow("Order alert", def).await;
    let flow_id = flow["id"].as_str().unwrap();
    let token = flow["trigger"]["token"].as_str().unwrap();

    let (status, resp) = app
        .req(
            "POST",
            &format!("/hooks/{token}"),
            Some(json!([
                {"amount": 250, "email": "a@x.io", "name": "ann lee"},
                {"amount": 20, "email": "b@x.io", "name": "bo chen"}
            ])),
        )
        .await;
    assert_eq!(status, 202, "{resp}");
    assert_eq!(resp["executions"].as_array().unwrap().len(), 2);

    app.drain().await;
    // Small order: email sent, now waiting on the delay. Big order: Slack-like call failed once → retry.
    let sent = app.mailer.sent.lock().unwrap().clone();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].subject, "Thanks Bo Chen");
    assert!(
        app.executions(flow_id)
            .await
            .iter()
            .all(|e| e["status"] == "waiting")
    );

    app.clock.advance(Duration::seconds(61));
    app.drain().await;
    app.clock.advance(Duration::days(1));
    app.drain().await;

    let runs = app.executions(flow_id).await;
    assert_eq!(runs.len(), 2);
    assert!(runs.iter().all(|e| e["status"] == "succeeded"), "{runs:#?}");
    let reviews = mock
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path() == "/review")
        .count();
    assert_eq!(reviews, 2);

    let big = runs
        .iter()
        .find(|e| e["task_count"] == 2)
        .expect("big order ran notify + review");
    let detail = app
        .ok(
            "GET",
            &format!("/api/v1/executions/{}", big["id"].as_str().unwrap()),
            None,
        )
        .await;
    let steps: Vec<(String, String)> = detail["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["step_key"].as_str().unwrap().to_owned(),
                s["status"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(
        steps,
        vec![
            ("is_big".into(), "succeeded".into()),
            ("notify".into(), "retry_scheduled".into()),
            ("notify".into(), "succeeded".into()),
            ("wait".into(), "running".into()),
            ("wait".into(), "succeeded".into()),
            ("review".into(), "succeeded".into()),
        ]
    );
    assert_eq!(detail["steps"][0]["output"]["branch"], "big");
    app.cleanup().await;
}

#[tokio::test]
async fn disabled_flows_ignore_webhooks_and_listening_captures_samples() {
    let app = TestApp::spawn().await;
    let flow = app
        .ok("POST", "/api/v1/flows", Some(json!({"name": "Draft only"})))
        .await;
    let id = flow["id"].as_str().unwrap();
    let token = flow["trigger"]["token"].as_str().unwrap();
    let (_, r) = app
        .req("POST", &format!("/hooks/{token}"), Some(json!({"a": 1})))
        .await;
    assert_eq!(r["status"], "ignored");
    app.ok("POST", &format!("/api/v1/flows/{id}/trigger/listen"), None)
        .await;
    let (_, r) = app
        .req(
            "POST",
            &format!("/hooks/{token}?x=1"),
            Some(json!({"email": "a@b.io"})),
        )
        .await;
    assert_eq!(r["status"], "sample_captured");
    let flow = app.ok("GET", &format!("/api/v1/flows/{id}"), None).await;
    assert_eq!(flow["trigger"]["sample"]["body"]["email"], "a@b.io");
    assert_eq!(flow["trigger"]["sample"]["query"]["x"], "1");
    let (status, _) = app.req("POST", "/hooks/not-a-token", Some(json!({}))).await;
    assert_eq!(status, 404);
    app.cleanup().await;
}
