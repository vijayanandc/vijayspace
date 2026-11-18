//! Logto token checks against a mocked tenant: opaque tokens (introspection + userinfo), JWTs
//! (JWKS), caching, and what is refused.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::{sync::Arc, time::Duration as StdDuration};

use chrono::{DateTime, TimeZone as _, Utc};
use jsonwebtoken::{EncodingKey, Header, encode};
use serde_json::json;
use weft_app::ports::{Clock, IdentityVerifier};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path},
};

use super::{LogtoSettings, LogtoVerifier};

/// A test-only P-256 key (not a secret); its public half is `JWK` below.
const TEST_KEY: &str = "-----BEGIN PRIVATE KEY-----
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgKFEtS/H1HNu4gk65
vBBU60h8+dx23vjm9rTjQrae63ahRANCAASIz/cMVt4BF1+ts4X4NN8tEdQd496T
83/ap2rDdk7uXfFXkn89iTgfWRmJAo2IJCyQFYCEeaPHK//BgKIYpjxg
-----END PRIVATE KEY-----
";

fn jwk() -> serde_json::Value {
    json!({"kty": "EC", "crv": "P-256", "kid": "test-key-1", "alg": "ES256", "use": "sig",
        "x": "iM_3DFbeARdfrbOF-DTfLRHUHePek_N_2qdqw3ZO7l0",
        "y": "8VeSfz2JOB9ZGYkCjYgkLJAVgIR5o8cr_8GAohimPGA"})
}

const NOW: i64 = 1_800_000_000;

#[derive(Debug)]
struct Fixed(DateTime<Utc>);
impl Clock for Fixed {
    fn now(&self) -> DateTime<Utc> {
        self.0
    }
}

fn verifier(server: &MockServer, resource: Option<&str>) -> LogtoVerifier {
    LogtoVerifier::new(
        LogtoSettings {
            endpoint: server.uri(),
            app_id: "server-app".into(),
            app_secret: "server-secret".into(),
            resource: resource.map(str::to_owned),
            client_ids: vec!["web-app".into()],
            required_scopes: vec!["openid".into(), "email".into()],
            cache_ttl: StdDuration::from_mins(5),
            cache_size: 100,
        },
        Arc::new(Fixed(Utc.timestamp_opt(NOW, 0).unwrap())),
    )
    .unwrap()
}

fn introspection(server_uri: &str, overrides: serde_json::Value) -> serde_json::Value {
    let mut body = json!({"active": true, "sub": "u1", "scope": "openid profile email offline_access",
        "iss": format!("{server_uri}/oidc"), "exp": NOW + 3600, "client_id": "web-app"});
    for (k, v) in overrides.as_object().unwrap() {
        body[k] = v.clone();
    }
    body
}

async fn mock_opaque(server: &MockServer, introspected: serde_json::Value, calls: u64) {
    Mock::given(method("POST"))
        .and(path("/oidc/token/introspection"))
        // Basic base64("server-app:server-secret")
        .and(header(
            "authorization",
            "Basic c2VydmVyLWFwcDpzZXJ2ZXItc2VjcmV0",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(introspected))
        .expect(calls)
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/oidc/me"))
        .and(header("authorization", "Bearer opaque-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"sub": "u1", "email": "Ann@Example.com", "email_verified": true, "name": "Ann"}),
        ))
        .mount(server)
        .await;
}

#[tokio::test]
async fn an_opaque_token_is_introspected_once_and_its_userinfo_becomes_the_identity() {
    let server = MockServer::start().await;
    mock_opaque(&server, introspection(&server.uri(), json!({})), 1).await;
    let v = verifier(&server, None);
    let id = v.verify("opaque-token").await.unwrap().unwrap();
    assert_eq!(id.subject, "u1");
    assert_eq!(id.email.as_deref(), Some("Ann@Example.com"));
    assert!(id.email_verified);
    assert_eq!(id.name.as_deref(), Some("Ann"));
    // Cached: the introspection mock expects exactly one call.
    assert_eq!(v.verify("opaque-token").await.unwrap(), Some(id));
}

#[tokio::test]
async fn opaque_tokens_logto_does_not_vouch_for_are_refused() {
    let cases = [
        json!({"active": false}),
        json!({"iss": "https://other.logto.app/oidc"}),
        json!({"client_id": "another-app"}),
        json!({"scope": "openid profile"}),
        json!({"exp": NOW - 1}),
        json!({"sub": ""}),
        json!({"iss": null}),
    ];
    for overrides in cases {
        let server = MockServer::start().await;
        mock_opaque(&server, introspection(&server.uri(), overrides.clone()), 1).await;
        assert_eq!(
            verifier(&server, None)
                .verify("opaque-token")
                .await
                .unwrap(),
            None,
            "{overrides}"
        );
    }
}

#[tokio::test]
async fn a_rejected_token_is_answered_locally_for_a_while() {
    let server = MockServer::start().await;
    // Expect exactly one introspection for three attempts.
    mock_opaque(
        &server,
        introspection(&server.uri(), json!({"active": false})),
        1,
    )
    .await;
    let v = verifier(&server, None);
    for _ in 0..3 {
        assert_eq!(v.verify("opaque-token").await.unwrap(), None);
    }
}

#[tokio::test]
async fn an_unreachable_provider_is_an_error_not_a_sign_out() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oidc/token/introspection"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let e = verifier(&server, None)
        .verify("opaque-token")
        .await
        .unwrap_err();
    assert_eq!(e.code, "auth.unavailable");
}

fn jwt(claims: &serde_json::Value, kid: &str) -> String {
    let mut h = Header::new(jsonwebtoken::Algorithm::ES256);
    h.kid = Some(kid.into());
    encode(
        &h,
        claims,
        &EncodingKey::from_ec_pem(TEST_KEY.as_bytes()).unwrap(),
    )
    .unwrap()
}

fn claims(server_uri: &str, overrides: serde_json::Value) -> serde_json::Value {
    let mut c = json!({"sub": "u2", "iss": format!("{server_uri}/oidc"), "aud": "https://api.weft.test",
        "exp": NOW + 900, "iat": NOW, "client_id": "web-app", "email": "bo@example.com",
        "email_verified": true, "name": "Bo"});
    for (k, v) in overrides.as_object().unwrap() {
        c[k] = v.clone();
    }
    c
}

async fn mock_jwks(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/oidc/jwks"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"keys": [jwk()]})))
        .mount(server)
        .await;
}

#[tokio::test]
async fn a_jwt_for_the_resource_is_verified_locally() {
    let server = MockServer::start().await;
    mock_jwks(&server).await;
    let v = verifier(&server, Some("https://api.weft.test"));
    let id = v
        .verify(&jwt(&claims(&server.uri(), json!({})), "test-key-1"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(id.subject, "u2");
    assert_eq!(id.email.as_deref(), Some("bo@example.com"));
    assert!(id.email_verified);
}

#[tokio::test]
async fn jwts_with_wrong_audience_issuer_time_client_or_key_are_refused() {
    let server = MockServer::start().await;
    mock_jwks(&server).await;
    let v = verifier(&server, Some("https://api.weft.test"));
    let uri = server.uri();
    let refused = [
        jwt(
            &claims(&uri, json!({"aud": "https://other.api"})),
            "test-key-1",
        ),
        jwt(
            &claims(&uri, json!({"iss": "https://evil.example/oidc"})),
            "test-key-1",
        ),
        jwt(&claims(&uri, json!({"exp": NOW - 61})), "test-key-1"),
        jwt(&claims(&uri, json!({"nbf": NOW + 120})), "test-key-1"),
        jwt(
            &claims(&uri, json!({"client_id": "another-app"})),
            "test-key-1",
        ),
        jwt(&claims(&uri, json!({})), "unknown-key"),
        "a.b.c".to_owned(),
    ];
    for token in refused {
        assert_eq!(v.verify(&token).await.unwrap(), None, "{token}");
    }
    // Within the 60 s leeway an expired token still passes.
    let late = jwt(&claims(&uri, json!({"exp": NOW - 30})), "test-key-1");
    assert!(v.verify(&late).await.unwrap().is_some());
    // Without a configured resource, JWTs are not accepted at all.
    let no_resource = verifier(&server, None);
    let token = jwt(&claims(&uri, json!({})), "test-key-1");
    assert_eq!(no_resource.verify(&token).await.unwrap(), None);
}
