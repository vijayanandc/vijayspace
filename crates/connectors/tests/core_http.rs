//! `core.http` against a mock server.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{sync::Arc, time::Duration};

use serde_json::json;
use weft_connector_sdk::{ActionCtx, Connector, ConnectorError};
use weft_connectors::core::http::HttpConnector;
use weft_http::{ReqwestSender, SsrfPolicy};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_json, header, method, path, query_param},
};

fn ctx() -> ActionCtx {
    ActionCtx {
        org_id: "org_test".into(),
        http: Arc::new(ReqwestSender::new(SsrfPolicy::allow_private()).unwrap()),
        connection: None,
        idempotency_key: "idem-1".into(),
        mailer: None,
        scripts: None,
        timeout: Duration::from_secs(5),
        now: chrono::Utc::now(),
    }
}

#[tokio::test]
async fn sends_json_with_headers_query_and_idempotency_key() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/items"))
        .and(query_param("debug", "1"))
        .and(header("x-api-key", "k"))
        .and(header("idempotency-key", "idem-1"))
        .and(body_json(json!({"name": "Ann"})))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"id": "i1"})))
        .expect(1)
        .mount(&server)
        .await;
    let input = json!({
        "method": "POST", "url": format!("{}/items", server.uri()),
        "headers": {"X-Api-Key": "k"}, "query": [{"key": "debug", "value": "1"}],
        "body": {"name": "Ann"}, "send_idempotency_key": true
    });
    let out = HttpConnector::new()
        .execute(&ctx(), "request", input)
        .await
        .unwrap();
    assert_eq!(out["status"], 201);
    assert_eq!(out["body"], json!({"id": "i1"}));
}

#[tokio::test]
async fn error_statuses_map_to_connector_errors() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/busy"))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "12"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/bad"))
        .respond_with(ResponseTemplate::new(400).set_body_string("nope"))
        .mount(&server)
        .await;
    let c = HttpConnector::new();
    let busy = c
        .execute(
            &ctx(),
            "request",
            json!({"method": "GET", "url": format!("{}/busy", server.uri())}),
        )
        .await
        .unwrap_err();
    assert_eq!(busy.retry_after_secs(), Some(12));
    let bad = c
        .execute(
            &ctx(),
            "request",
            json!({"method": "GET", "url": format!("{}/bad", server.uri())}),
        )
        .await
        .unwrap_err();
    assert!(matches!(bad, ConnectorError::InvalidInput { .. }));
    Mock::given(method("GET"))
        .and(path("/locked"))
        .respond_with(ResponseTemplate::new(401).set_body_string("{\"error\":\"Unauthorized\"}"))
        .mount(&server)
        .await;
    let locked = c
        .execute(
            &ctx(),
            "request",
            json!({"method": "GET", "url": format!("{}/locked", server.uri())}),
        )
        .await
        .unwrap_err();
    assert_eq!(
        locked.code(),
        "app.invalid_input",
        "no connection to sign in to again: the request's headers are wrong"
    );
    let tolerant = c.execute(&ctx(), "request", json!({"method": "GET", "url": format!("{}/bad", server.uri()), "fail_on_error_status": false})).await.unwrap();
    assert_eq!(tolerant["status"], 400);
    assert_eq!(tolerant["body"], "nope");
}

#[test]
fn only_methods_that_http_defines_as_idempotent_are_safe_to_repeat() {
    use weft_connector_sdk::Connector as _;
    let http = HttpConnector::new();
    for (method, safe) in [
        ("GET", true),
        ("put", true),
        ("DELETE", true),
        ("POST", false),
        ("PATCH", false),
    ] {
        assert_eq!(
            http.is_idempotent("request", &json!({"method": method})),
            safe,
            "{method}"
        );
    }
    assert!(
        http.is_idempotent("request", &json!({})),
        "GET is the default method"
    );
}
