//! Connections: the OAuth start keeps only the app's declared settings, so callers cannot steer
//! the token URL (which receives the client secret) or the API domain.

use serde_json::json;
use sha2::{Digest, Sha256};

use crate::harness::TestApp;

#[tokio::test]
async fn oauth_start_keeps_only_declared_settings() {
    let app = TestApp::spawn().await;
    let started = app
        .ok("POST", "/api/v1/connections/oauth/start", Some(json!({
            "connector": "zoho_crm", "name": "CRM", "return_to": "http://localhost:5173/connections/callback",
            "fields": {"edition": "enterprise", "accounts_server": "https://attacker.example", "api_domain": "https://evil.example"}
        })))
        .await;
    let url = started["authorize_url"].as_str().unwrap();
    assert!(
        url.starts_with("https://accounts.zoho.com/oauth/v2/auth"),
        "{url}"
    );
    let state = url
        .split(['?', '&'])
        .find_map(|p| p.strip_prefix("state="))
        .unwrap();
    let hash = Sha256::digest(state.as_bytes()).to_vec();
    let now = weft_app::ports::Clock::now(&app.clock);
    let stored = app
        .deps
        .store
        .take_oauth_state(&hash, now)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.config, json!({"edition": "enterprise"}));

    let (status, body) = app
        .req(
            "POST",
            "/api/v1/connections/oauth/start",
            Some(json!({
                "connector": "zoho_crm", "name": "CRM", "return_to": "http://localhost:5173/x",
                "fields": {"edition": "platinum"}
            })),
        )
        .await;
    assert_eq!(status.as_u16(), 422, "{body}");
    app.cleanup().await;
}

#[tokio::test]
async fn concurrent_forced_refreshes_call_the_token_endpoint_once() {
    use weft_app::connections::handle::{DbConnectionHandle, encrypt};
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    let app = TestApp::spawn().await;
    let accounts = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth/v2/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "new-token", "expires_in": 3600, "api_domain": accounts.uri()
        })))
        .expect(1)
        .mount(&accounts)
        .await;
    app.ok("GET", "/api/v1/org", None).await;
    let now = weft_app::ports::Clock::now(&app.clock);
    let connection = weft_domain::model::Connection {
        id: app.deps.new_connection_id(),
        org_id: weft_domain::ids::OrgId::from_uuid(weft_server::wiring::DEFAULT_ORG),
        connector: "zoho_crm".into(),
        name: "CRM".into(),
        config: json!({"accounts_server": accounts.uri()}),
        status: weft_domain::model::ConnectionStatus::Active,
        created_at: now,
        updated_at: now,
    };
    let secrets = json!({"access_token": "old-token", "refresh_token": "r1",
        "expires_at": (now + chrono::Duration::hours(1)).to_rfc3339()});
    let (ciphertext, key_id) =
        encrypt(&app.deps, &connection, secrets.as_object().unwrap()).unwrap();
    let stored = weft_app::ports::StoredConnection {
        connection,
        ciphertext,
        key_id,
    };
    app.deps.store.insert_connection(&stored).await.unwrap();

    // Eight steps got a 401 with the same old token at the same time.
    let handles: Vec<_> = (0..8)
        .map(|_| DbConnectionHandle::new(app.deps.clone(), stored.clone()))
        .collect();
    let tokens = futures::future::join_all(
        handles
            .iter()
            .map(weft_connector_sdk::ConnectionHandle::force_refresh),
    )
    .await;
    for token in tokens {
        assert_eq!(token.unwrap(), "new-token");
    }
    accounts.verify().await;
    app.cleanup().await;
}
