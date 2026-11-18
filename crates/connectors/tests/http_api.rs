//! `http_api` ("Any app") against a mock API: every way of signing requests, the client-credentials token cache,
//! the connection test, paths that try to leave the base URL, and the "New items" polling trigger.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use serde_json::{Value, json};
use weft_connector_sdk::{
    ActionCtx, Connector, ConnectorError,
    testing::{StaticConnection, ctx},
};
use weft_connectors::http_api::HttpApiConnector;
use weft_http::{ReqwestSender, SsrfPolicy};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_string_contains, header, method, path, query_param},
};

fn app_ctx(config: Value, secrets: Value) -> ActionCtx {
    ctx(
        Arc::new(ReqwestSender::new(SsrfPolicy::allow_private()).unwrap()),
        Some(StaticConnection::new(config, secrets)),
    )
}

async fn ok_on(server: &MockServer, matcher: impl wiremock::Match + 'static) {
    Mock::given(method("GET"))
        .and(path("/v1/orders/7"))
        .and(matcher)
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": 7})))
        .expect(1)
        .mount(server)
        .await;
}

async fn call(cx: &ActionCtx) -> Result<Value, ConnectorError> {
    HttpApiConnector::new()
        .execute(
            cx,
            "api_call",
            json!({"method": "GET", "path": "/orders/7"}),
        )
        .await
}

#[tokio::test]
async fn each_way_of_signing_puts_the_credentials_where_the_app_expects_them() {
    let server = MockServer::start().await;
    let base = format!("{}/v1", server.uri());
    ok_on(&server, header("x-shop-key", "k1")).await;
    let cx = app_ctx(
        json!({"base_url": base, "auth": "api_key_header", "header_name": "X-Shop-Key"}),
        json!({"api_key": "k1"}),
    );
    assert_eq!(call(&cx).await.unwrap()["body"]["id"], 7);
    server.reset().await;

    ok_on(&server, query_param("key", "k2")).await;
    let cx = app_ctx(
        json!({"base_url": base, "auth": "api_key_query", "query_name": "key"}),
        json!({"api_key": "k2"}),
    );
    assert_eq!(call(&cx).await.unwrap()["status"], 200);
    server.reset().await;

    ok_on(&server, header("authorization", "Bearer t3")).await;
    let cx = app_ctx(
        json!({"base_url": base, "auth": "bearer"}),
        json!({"token": "t3"}),
    );
    assert_eq!(call(&cx).await.unwrap()["status"], 200);
    server.reset().await;

    // "ann:pw4" in base64.
    ok_on(&server, header("authorization", "Basic YW5uOnB3NA==")).await;
    let cx = app_ctx(
        json!({"base_url": base, "auth": "basic", "username": "ann", "default_headers": {"Accept": "application/json"}}),
        json!({"password": "pw4"}),
    );
    assert_eq!(call(&cx).await.unwrap()["status"], 200);
    server.verify().await;
}

#[tokio::test]
async fn client_credentials_tokens_are_cached_and_fetched_again_after_a_401() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth/token"))
        .and(body_string_contains("grant_type=client_credentials"))
        .and(body_string_contains("client_secret=s3cret"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"access_token": "tok-1", "expires_in": 3600})),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/oauth/token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"access_token": "tok-2", "expires_in": 3600})),
        )
        .mount(&server)
        .await;
    // tok-1 works twice, then the app revokes it.
    Mock::given(method("GET"))
        .and(path("/v1/orders/7"))
        .and(header("authorization", "Bearer tok-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": 7})))
        .up_to_n_times(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/orders/7"))
        .and(header("authorization", "Bearer tok-1"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/orders/7"))
        .and(header("authorization", "Bearer tok-2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": 7, "fresh": true})))
        .mount(&server)
        .await;
    let cx = app_ctx(
        json!({"base_url": format!("{}/v1", server.uri()), "auth": "oauth2_client_credentials",
            "token_url": format!("{}/oauth/token", server.uri()), "client_id": "weft"}),
        json!({"client_secret": "s3cret"}),
    );
    let app = HttpApiConnector::new();
    let input = json!({"path": "/orders/7"});
    app.execute(&cx, "api_call", input.clone()).await.unwrap();
    app.execute(&cx, "api_call", input.clone()).await.unwrap();
    let third = app.execute(&cx, "api_call", input).await.unwrap();
    assert_eq!(
        third["body"]["fresh"], true,
        "a refused token is replaced once"
    );
    let token_calls = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path() == "/oauth/token")
        .count();
    assert_eq!(
        token_calls, 2,
        "cached between calls, fetched again only after the 401"
    );
}

#[tokio::test]
async fn the_connection_test_and_paths_never_leave_the_base_url() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/me"))
        .and(header("authorization", "Bearer good"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"name": "Acme"})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/me"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    let base = format!("{}/v1", server.uri());
    let app = HttpApiConnector::new();
    let good = app_ctx(
        json!({"base_url": base, "auth": "bearer", "test_path": "/me"}),
        json!({"token": "good"}),
    );
    assert_eq!(app.test_connection(&good).await.unwrap()["status"], 200);
    let bad = app_ctx(
        json!({"base_url": base, "auth": "bearer", "test_path": "/me"}),
        json!({"token": "bad"}),
    );
    assert!(matches!(
        app.test_connection(&bad).await.unwrap_err(),
        ConnectorError::AuthExpired { .. }
    ));
    for escape in ["https://attacker.example/steal", "//attacker.example/steal"] {
        let err = app
            .execute(&good, "api_call", json!({"path": escape}))
            .await
            .unwrap_err();
        assert!(
            matches!(err, ConnectorError::InvalidInput { .. }),
            "{escape}: {err:?}"
        );
    }
    assert!(app.is_idempotent("api_call", &json!({"method": "GET"})));
    assert!(!app.is_idempotent("api_call", &json!({"method": "POST"})));
}

#[tokio::test]
async fn new_items_start_runs_only_for_items_not_seen_before() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/orders"))
        .and(header("x-api-key", "k"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"data": {"orders": [{"id": 2}, {"id": 1}]}})),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/orders"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"data": {"orders": [{"id": 3, "total": 9}, {"id": 2}, {"id": 1}]}}),
        ))
        .mount(&server)
        .await;
    let cx = app_ctx(
        json!({"base_url": format!("{}/v1", server.uri()), "auth": "api_key_header"}),
        json!({"api_key": "k"}),
    );
    let app = HttpApiConnector::new();
    let input = json!({"path": "/orders", "items_path": "data.orders", "id_field": "id"});
    let baseline = app.poll(&cx, "new_items", &input, None).await.unwrap();
    assert!(
        baseline.items.is_empty(),
        "the first poll remembers what is there"
    );
    let next = app
        .poll(&cx, "new_items", &input, baseline.cursor.as_ref())
        .await
        .unwrap();
    assert_eq!(next.items.len(), 1);
    assert_eq!(next.items[0].dedupe_key, "item:3");
    assert_eq!(next.items[0].data["item"]["total"], 9);
    assert_eq!(next.cursor, Some(json!({"seen": ["3", "2", "1"]})));
    let wrong = json!({"path": "/orders", "items_path": "nope"});
    assert!(
        app.poll(&cx, "new_items", &wrong, next.cursor.as_ref())
            .await
            .is_err()
    );
}

#[test]
fn only_the_fields_of_the_chosen_sign_in_apply() {
    let app = HttpApiConnector::new();
    let fields = &app.manifest().auth.fields;
    let shown = |auth: &str| -> Vec<String> {
        let values = json!({"auth": auth});
        let values = values.as_object().unwrap();
        fields
            .iter()
            .filter(|f| f.applies(values, fields))
            .map(|f| f.key.clone())
            .collect()
    };
    assert_eq!(
        shown("bearer"),
        ["base_url", "auth", "token", "test_path", "default_headers"]
    );
    assert!(shown("basic").contains(&"password".to_owned()));
    assert!(!shown("basic").contains(&"api_key".to_owned()));
    // No choice yet: the default (API key in a header) applies.
    let none = serde_json::Map::new();
    assert!(
        fields
            .iter()
            .any(|f| f.key == "header_name" && f.applies(&none, fields))
    );
}
