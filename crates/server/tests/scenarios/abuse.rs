//! Limits that keep one org's flows from harming the platform (anyone signed in can create an org): emails sent by
//! flows (recipients per message, messages per day) and template size and output.

use serde_json::{Value, json};
use weft_domain::ids::OrgId;

use crate::harness::TestApp;

async fn test_step(app: &TestApp, input: Value) -> Value {
    let def = json!({"trigger": {"type": "manual"}, "steps": [
        {"key": "step", "type": "action", "connector": "core.email", "action": "send", "input": input}
    ]});
    let flow = app
        .ok(
            "POST",
            "/api/v1/flows",
            Some(json!({"name": "Mail", "definition": def})),
        )
        .await;
    let id = flow["id"].as_str().unwrap();
    app.ok(
        "POST",
        &format!("/api/v1/flows/{id}/steps/step/test"),
        Some(json!({})),
    )
    .await
}

#[tokio::test]
async fn flows_send_email_to_a_few_recipients_and_a_daily_number_of_messages() {
    let app = TestApp::spawn().await;
    let ok = test_step(
        &app,
        json!({"to": "a@example.com", "subject": "Hi", "body": "Hello"}),
    )
    .await;
    assert!(ok.get("error").is_none_or(Value::is_null), "{ok}");
    assert_eq!(app.mailer.sent.lock().unwrap().len(), 1);
    let many: Vec<String> = (0..21).map(|i| format!("u{i}@example.com")).collect();
    let refused = test_step(&app, json!({"to": many, "subject": "Hi", "body": "Hello"})).await;
    assert!(
        refused["error"]["message"]
            .as_str()
            .unwrap()
            .contains("at most 20"),
        "{refused}"
    );
    // The rest of today's quota goes (199 more).
    let org = OrgId::from_uuid(weft_server::wiring::DEFAULT_ORG);
    let now = app.deps.clock.now();
    for _ in 0..199 {
        let quota = [(
            format!("email:org:{org}"),
            weft_app::limits::FLOW_EMAILS_PER_DAY,
        )];
        assert!(
            app.deps
                .store
                .take_send_quota(&quota, now.date_naive(), now)
                .await
                .unwrap()
        );
    }
    let over = test_step(
        &app,
        json!({"to": "a@example.com", "subject": "Hi", "body": "Hello"}),
    )
    .await;
    assert!(
        over["error"]["message"]
            .as_str()
            .unwrap()
            .contains("daily limit"),
        "{over}"
    );
    assert_eq!(
        app.mailer.sent.lock().unwrap().len(),
        1,
        "nothing more went out"
    );
    app.cleanup().await;
}

#[tokio::test]
async fn templates_that_build_up_state_are_refused_when_the_flow_is_saved() {
    let app = TestApp::spawn().await;
    let def = json!({"trigger": {"type": "manual"}, "steps": [
        {"key": "s", "type": "set_variable", "assignments": [
            {"name": "x", "value": "{% set a = 'x' %}{% for i in range(40) %}{% set a = a ~ a %}{% endfor %}{{ a }}"}
        ]}
    ]});
    let flow = app
        .ok(
            "POST",
            "/api/v1/flows",
            Some(json!({"name": "Grow", "definition": def})),
        )
        .await;
    let id = flow["id"].as_str().unwrap();
    let issues = app
        .ok("POST", &format!("/api/v1/flows/{id}/validate"), None)
        .await;
    let text = issues.to_string();
    assert!(
        text.contains("set") && text.contains("not supported"),
        "{text}"
    );
    app.cleanup().await;
}

#[tokio::test]
async fn a_step_uses_only_a_connection_of_its_own_app() {
    let app = TestApp::spawn().await;
    let src = app.connection("memsrc", "Source").await;
    let dst = app.connection("memdst", "Target").await;
    let step = |connection: &str| {
        json!({"trigger": {"type": "manual"}, "steps": [
            {"key": "step", "type": "action", "connector": "memdst", "action": "create_record",
             "connection_id": connection, "input": {"module": "contacts", "values": {"name": "Ann"}}}
        ]})
    };
    for (connection, ok) in [(&src, false), (&dst, true)] {
        let flow = app
            .ok(
                "POST",
                "/api/v1/flows",
                Some(json!({"name": "F", "definition": step(connection)})),
            )
            .await;
        let id = flow["id"].as_str().unwrap();
        let res = app
            .ok(
                "POST",
                &format!("/api/v1/flows/{id}/steps/step/test"),
                Some(json!({})),
            )
            .await;
        if ok {
            assert!(res.get("error").is_none_or(Value::is_null), "{res}");
        } else {
            assert!(
                res["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("This connection is for memsrc"),
                "another app's credentials are never used: {res}"
            );
        }
    }
    app.cleanup().await;
}
