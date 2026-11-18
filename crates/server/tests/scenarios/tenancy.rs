//! Sign-in and tenancy (doc 13 §6, doc 08 §2–3): tokens, users created on first sign-in, orgs,
//! isolation between orgs, invitations, and the role matrix through the real API.

use serde_json::{Value, json};
use weft_app::ports::VerifiedIdentity;

use crate::harness::TestApp;

const WEBHOOK_FLOW: &str = r#"{"name": "F", "definition": {"trigger": {"type": "webhook"}, "steps": [{"key": "s", "type": "stop"}]}}"#;

fn idp(app: &TestApp) -> &weft_testkit::FakeIdentityProvider {
    app.idp.as_ref().unwrap()
}

/// Signs `email` in (token = email) and creates an org they own; returns the org ID.
async fn org_of(app: &TestApp, email: &str, name: &str) -> String {
    idp(app).issue(email, email, name);
    let (status, org) = app
        .req_as(
            email,
            None,
            "POST",
            "/api/v1/orgs",
            Some(json!({"name": format!("{name}'s org")})),
        )
        .await;
    assert_eq!(status.as_u16(), 201, "{org}");
    assert_eq!(org["role"], "owner");
    org["id"].as_str().unwrap().to_owned()
}

async fn as_user(
    app: &TestApp,
    who: &str,
    org: &str,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> (u16, Value) {
    let (status, json) = app.req_as(who, Some(org), method, path, body).await;
    (status.as_u16(), json)
}

#[tokio::test]
async fn requests_need_a_valid_sign_in_and_the_client_learns_how_to_sign_in() {
    let app = TestApp::spawn_signed_in().await;
    let config = app.ok("GET", "/api/v1/auth/config", None).await;
    assert_eq!(config["sign_in"]["app_id"], "web-app");
    assert_eq!(config["sign_in"]["endpoint"], "https://tenant.logto.test");
    for path in ["/api/v1/me", "/api/v1/flows", "/api/v1/org"] {
        let (status, body) = app.req("GET", path, None).await;
        assert_eq!(
            (status.as_u16(), body["code"].as_str()),
            (401, Some("auth.unauthenticated")),
            "{path}"
        );
    }
    let (status, _) = app
        .req_as("not-issued", None, "GET", "/api/v1/me", None)
        .await;
    assert_eq!(status.as_u16(), 401);
    idp(&app).issue("t1", "ann@example.com", "Ann");
    let me = app.req_as("t1", None, "GET", "/api/v1/me", None).await.1;
    assert_eq!(me["user"]["email"], "ann@example.com");
    assert!(
        me["user"].get("logto_subject").is_none(),
        "the Logto subject stays internal"
    );
    assert_eq!(me["orgs"], json!([]), "a new user belongs to no org yet");
    idp(&app).revoke("t1");
    assert_eq!(
        app.req_as("t1", None, "GET", "/api/v1/me", None)
            .await
            .0
            .as_u16(),
        401
    );
    idp(&app).set_down(true);
    let (status, body) = app.req_as("t1", None, "GET", "/api/v1/me", None).await;
    assert_eq!(
        (status.as_u16(), body["code"].as_str()),
        (502, Some("auth.unavailable"))
    );
    app.cleanup().await;
}

#[tokio::test]
async fn owners_listed_in_the_settings_own_the_default_org_and_users_create_their_own() {
    let app = TestApp::spawn_signed_in().await;
    idp(&app).issue("own", "Owner@Weft.test", "Olu");
    let me = app.req_as("own", None, "GET", "/api/v1/me", None).await.1;
    assert_eq!(
        me["orgs"][0]["id"],
        json!(weft_domain::ids::OrgId::from_uuid(
            weft_server::wiring::DEFAULT_ORG
        ))
    );
    assert_eq!(me["orgs"][0]["role"], "owner");
    let org = org_of(&app, "bo@example.com", "Bo").await;
    let (status, body) = app
        .req_as("bo@example.com", None, "GET", "/api/v1/flows", None)
        .await;
    assert_eq!(
        (status.as_u16(), body["code"].as_str()),
        (422, Some("validation.failed")),
        "X-Org-ID is required"
    );
    let (status, flows) = as_user(&app, "bo@example.com", &org, "GET", "/api/v1/flows", None).await;
    assert_eq!((status, flows), (200, json!([])));
    let (status, body) = app
        .req_as(
            "bo@example.com",
            None,
            "POST",
            "/api/v1/orgs",
            Some(json!({"name": " "})),
        )
        .await;
    assert_eq!(status.as_u16(), 422, "{body}");
    let (status, _) = app
        .req_as(
            "bo@example.com",
            None,
            "POST",
            "/api/v1/orgs",
            Some(json!({"name": "X", "timezone": "Mars/Base"})),
        )
        .await;
    assert_eq!(status.as_u16(), 422);
    app.cleanup().await;
}

#[tokio::test]
async fn another_orgs_data_is_not_found_never_forbidden() {
    let app = TestApp::spawn_signed_in().await;
    let a = org_of(&app, "ann@example.com", "Ann").await;
    let b = org_of(&app, "bo@example.com", "Bo").await;
    let flow: Value = serde_json::from_str(WEBHOOK_FLOW).unwrap();
    let (_, created) = as_user(
        &app,
        "bo@example.com",
        &b,
        "POST",
        "/api/v1/flows",
        Some(flow),
    )
    .await;
    let flow_id = created["id"].as_str().unwrap();
    // Ann, acting in Bo's org: she is not a member.
    let (status, body) = as_user(&app, "ann@example.com", &b, "GET", "/api/v1/flows", None).await;
    assert_eq!(
        (status, body["code"].as_str()),
        (404, Some("org.not_found"))
    );
    // Ann, in her own org, naming Bo's flow.
    for (method, path) in [
        ("GET", format!("/api/v1/flows/{flow_id}")),
        ("DELETE", format!("/api/v1/flows/{flow_id}")),
    ] {
        let (status, body) = as_user(&app, "ann@example.com", &a, method, &path, None).await;
        assert_eq!(status, 404, "{method} {path}: {body}");
        assert!(body.get("name").is_none(), "no data leaks: {body}");
    }
    let (status, _) = as_user(
        &app,
        "ann@example.com",
        "org_garbage",
        "GET",
        "/api/v1/flows",
        None,
    )
    .await;
    assert_eq!(status, 404);
    // Webhooks stay public (their token is the credential).
    let (_, flows) = as_user(&app, "bo@example.com", &b, "GET", "/api/v1/flows", None).await;
    assert_eq!(flows.as_array().unwrap().len(), 1);
    app.cleanup().await;
}

#[tokio::test]
async fn invitations_go_out_by_email_and_bind_to_the_verified_address() {
    let app = TestApp::spawn_signed_in().await;
    let org = org_of(&app, "ann@example.com", "Ann").await;
    let (status, invited) = as_user(
        &app,
        "ann@example.com",
        &org,
        "POST",
        "/api/v1/invitations",
        Some(json!({"email": " Cy@Example.com ", "role": "admin"})),
    )
    .await;
    assert_eq!(status, 201, "{invited}");
    assert_eq!(invited["email_sent"], true);
    assert_eq!(invited["invitation"]["email"], "cy@example.com");
    assert_eq!(invited["invitation"]["invited_by"], "Ann");
    let mail = app.mailer.sent.lock().unwrap().last().cloned().unwrap();
    assert_eq!(mail.to, vec!["cy@example.com"]);
    assert!(mail.subject.contains("Ann's org") && mail.body.contains("http://app.weft.test/"));
    let id = invited["invitation"]["id"].as_str().unwrap().to_owned();
    // Someone else (or an unverified address) cannot take it.
    idp(&app).issue("dee", "dee@example.com", "Dee");
    let (status, _) = app
        .req_as(
            "dee",
            None,
            "POST",
            &format!("/api/v1/me/invitations/{id}/accept"),
            None,
        )
        .await;
    assert_eq!(status.as_u16(), 404);
    idp(&app).issue_identity(
        "cy-unverified",
        VerifiedIdentity {
            subject: "sub-cy2".into(),
            email: Some("cy@example.com".into()),
            email_verified: false,
            name: None,
            has_profile: true,
        },
    );
    let (status, _) = app
        .req_as(
            "cy-unverified",
            None,
            "POST",
            &format!("/api/v1/me/invitations/{id}/accept"),
            None,
        )
        .await;
    assert_eq!(status.as_u16(), 403);
    // Cy sees it after signing in, and accepts.
    idp(&app).issue("cy", "cy@example.com", "Cy");
    let me = app.req_as("cy", None, "GET", "/api/v1/me", None).await.1;
    assert_eq!(me["invitations"][0]["org_name"], "Ann's org");
    let joined = app
        .req_as(
            "cy",
            None,
            "POST",
            &format!("/api/v1/me/invitations/{id}/accept"),
            None,
        )
        .await
        .1;
    assert_eq!(joined["role"], "admin");
    let (_, members) = as_user(&app, "cy", &org, "GET", "/api/v1/members", None).await;
    assert_eq!(members.as_array().unwrap().len(), 2);
    let (status, _) = as_user(
        &app,
        "ann@example.com",
        &org,
        "POST",
        "/api/v1/invitations",
        Some(json!({"email": "cy@example.com", "role": "member"})),
    )
    .await;
    assert_eq!(status, 409, "already a member");
    // Declining removes the invitation.
    let (_, again) = as_user(
        &app,
        "ann@example.com",
        &org,
        "POST",
        "/api/v1/invitations",
        Some(json!({"email": "dee@example.com", "role": "viewer"})),
    )
    .await;
    let dee_inv = again["invitation"]["id"].as_str().unwrap();
    let (status, _) = app
        .req_as(
            "dee",
            None,
            "POST",
            &format!("/api/v1/me/invitations/{dee_inv}/decline"),
            None,
        )
        .await;
    assert_eq!(status.as_u16(), 204);
    let (_, open) = as_user(
        &app,
        "ann@example.com",
        &org,
        "GET",
        "/api/v1/invitations",
        None,
    )
    .await;
    assert_eq!(open, json!([]));
    app.cleanup().await;
}
