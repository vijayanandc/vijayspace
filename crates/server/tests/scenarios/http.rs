//! HTTP conventions through the real router: every error is problem+json with a stable code, and every response
//! carries a request ID.

use axum::{body::Body, http::Request};
use http_body_util::BodyExt as _;
use serde_json::Value;
use tower::ServiceExt as _;

use crate::harness::TestApp;

async fn raw(
    app: &TestApp,
    method: &str,
    path: &str,
    body: &str,
) -> (u16, String, Option<String>, Value) {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(body.to_owned()))
        .unwrap();
    let response = app.router.clone().oneshot(request).await.unwrap();
    let status = response.status().as_u16();
    let kind = response
        .headers()
        .get("content-type")
        .map(|v| v.to_str().unwrap().to_owned())
        .unwrap_or_default();
    let id = response
        .headers()
        .get("x-request-id")
        .map(|v| v.to_str().unwrap().to_owned());
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        kind,
        id,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn errors_are_problem_json_and_responses_carry_a_request_id() {
    let app = TestApp::spawn().await;
    for (method, path, body, status) in [
        ("POST", "/api/v1/flows", "{not json", 400),
        ("GET", "/api/v1/no-such-thing", "", 404),
        ("GET", "/api/v1/executions?limit=lots", "", 400),
    ] {
        let (got, kind, id, json) = raw(&app, method, path, body).await;
        assert_eq!(got, status, "{method} {path}: {json}");
        assert_eq!(kind, "application/problem+json", "{method} {path}");
        assert_eq!(json["code"], format!("http.{status}"), "{json}");
        assert!(
            id.is_some_and(|i| i.len() >= 32),
            "a request ID on every response"
        );
    }
    let (status, kind, _, json) = raw(
        &app,
        "GET",
        "/api/v1/flows/flw_00000000000070008000000000000099",
        "",
    )
    .await;
    assert_eq!((status, kind.as_str()), (404, "application/problem+json"));
    assert_eq!(
        json["code"], "flow.not_found",
        "the app's own codes are kept"
    );
    app.cleanup().await;
}
