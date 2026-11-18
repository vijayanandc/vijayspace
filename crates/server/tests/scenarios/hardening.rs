//! Security fixes from the sign-in review: members cannot redirect or take over others'
//! connections, settings changes need the credentials again, a removed bootstrap owner stays
//! removed, invitations keep their rules, viewers see no step inputs, and the catalog needs sign-in.

use chrono::Duration;
use serde_json::json;
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::any};

use crate::{
    harness::TestApp,
    roles::{ADMIN, MEMBER, OWNER, Team, VIEWER, call, team},
};

#[tokio::test]
async fn members_cannot_redirect_another_members_connection_and_changes_need_the_credentials() {
    let app = TestApp::spawn_signed_in().await;
    let Team { org } = team(&app).await;
    let legit = MockServer::start().await;
    let attacker = MockServer::start().await;
    for server in [&legit, &attacker] {
        Mock::given(any())
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": []})))
            .mount(server)
            .await;
    }
    let (_, c) = call(
        &app,
        OWNER,
        &org,
        "POST",
        "/api/v1/connections",
        Some(json!({"connector": "appblocks", "name": "FSM",
        "fields": {"base_url": legit.uri(), "org_id": "1", "api_token": "apb_OWNER_SECRET"}})),
    )
    .await;
    let id = c["id"].as_str().unwrap().to_owned();
    let path = format!("/api/v1/connections/{id}");
    let redirect = json!({"fields": {"base_url": attacker.uri()}});
    let (status, _) = call(&app, MEMBER, &org, "PATCH", &path, Some(redirect.clone())).await;
    assert_eq!(
        status, 403,
        "a member cannot change someone else's connection"
    );
    let (status, _) = call(
        &app,
        MEMBER,
        &org,
        "PUT",
        &format!("{path}/limits"),
        Some(json!({"requests_per_minute": 1})),
    )
    .await;
    assert_eq!(status, 403);
    let (status, body) = call(&app, OWNER, &org, "PATCH", &path, Some(redirect)).await;
    assert_eq!(
        status, 422,
        "the stored token must not follow a new base URL: {body}"
    );
    assert!(body["detail"].as_str().unwrap().contains("again"), "{body}");
    call(&app, MEMBER, &org, "POST", &format!("{path}/test"), None).await;
    assert!(
        attacker
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty(),
        "the secret never reached the new host"
    );
    let (status, _) = call(
        &app,
        OWNER,
        &org,
        "PATCH",
        &path,
        Some(json!({"fields": {"base_url": attacker.uri(), "api_token": "apb_NEW"}})),
    )
    .await;
    assert_eq!(status, 200, "with the credentials entered again");
    let (status, _) = call(
        &app,
        ADMIN,
        &org,
        "PATCH",
        &path,
        Some(json!({"name": "FSM (prod)"})),
    )
    .await;
    assert_eq!(status, 200, "admins manage every connection");
    app.cleanup().await;
}

#[tokio::test]
async fn a_removed_bootstrap_owner_stays_removed() {
    let app = TestApp::spawn_signed_in().await;
    let idp = app.idp.as_ref().unwrap();
    idp.issue("boot", "owner@weft.test", "Boot");
    idp.issue("two", "two@weft.test", "Two");
    let me = app.req_as("boot", None, "GET", "/api/v1/me", None).await.1;
    let org = me["orgs"][0]["id"].as_str().unwrap().to_owned();
    let (_, invited) = call(
        &app,
        "boot",
        &org,
        "POST",
        "/api/v1/invitations",
        Some(json!({"email": "two@weft.test", "role": "owner"})),
    )
    .await;
    let inv = invited["invitation"]["id"].as_str().unwrap();
    app.req_as(
        "two",
        None,
        "POST",
        &format!("/api/v1/me/invitations/{inv}/accept"),
        None,
    )
    .await;
    let boot_id = me["user"]["id"].as_str().unwrap();
    assert_eq!(
        call(
            &app,
            "two",
            &org,
            "DELETE",
            &format!("/api/v1/members/{boot_id}"),
            None
        )
        .await
        .0,
        204
    );
    let again = app.req_as("boot", None, "GET", "/api/v1/me", None).await.1;
    assert_eq!(again["orgs"], json!([]), "not re-added as owner: {again}");
    app.cleanup().await;
}

#[tokio::test]
async fn invitations_keep_their_rules() {
    let app = TestApp::spawn_signed_in().await;
    let Team { org } = team(&app).await;
    let (status, invited) = call(
        &app,
        OWNER,
        &org,
        "POST",
        "/api/v1/invitations",
        Some(json!({"email": "boss@example.com", "role": "owner"})),
    )
    .await;
    assert_eq!(status, 201);
    let (status, _) = call(
        &app,
        ADMIN,
        &org,
        "POST",
        "/api/v1/invitations",
        Some(json!({"email": "boss@example.com", "role": "viewer"})),
    )
    .await;
    assert_eq!(status, 403, "an admin cannot rewrite an owner's invitation");
    let id = invited["invitation"]["id"].as_str().unwrap();
    let resend = format!("/api/v1/invitations/{id}/resend");
    let (status, body) = call(&app, OWNER, &org, "POST", &resend, None).await;
    assert_eq!(
        (status, body["code"].as_str()),
        (409, Some("invitation.rate_limited"))
    );
    app.clock.advance(Duration::minutes(6));
    assert_eq!(call(&app, OWNER, &org, "POST", &resend, None).await.0, 200);
    // Revoked invitations still count toward the day's emails, and cannot be accepted.
    assert_eq!(
        call(
            &app,
            OWNER,
            &org,
            "DELETE",
            &format!("/api/v1/invitations/{id}"),
            None
        )
        .await
        .0,
        204
    );
    let (_, open) = call(&app, OWNER, &org, "GET", "/api/v1/invitations", None).await;
    assert!(
        open.as_array()
            .unwrap()
            .iter()
            .all(|i| i["email"] != "boss@example.com"),
        "{open}"
    );
    app.idp
        .as_ref()
        .unwrap()
        .issue("boss", "boss@example.com", "Boss");
    let (status, _) = app
        .req_as(
            "boss",
            None,
            "POST",
            &format!("/api/v1/me/invitations/{id}/accept"),
            None,
        )
        .await;
    assert_eq!(
        status.as_u16(),
        404,
        "a revoked invitation cannot be accepted"
    );
    app.cleanup().await;
}

#[tokio::test]
async fn viewers_see_no_step_inputs_and_the_catalog_needs_sign_in() {
    let app = TestApp::spawn_signed_in().await;
    let Team { org } = team(&app).await;
    let flow = json!({"name": "F", "definition": {"trigger": {"type": "manual"}, "steps": [
        {"key": "call", "type": "action", "connector": "core.http", "action": "request",
         "input": {"method": "GET", "url": "https://example.com", "headers": {"Authorization": "Bearer typed-secret"}}}]}});
    let (_, created) = call(&app, OWNER, &org, "POST", "/api/v1/flows", Some(flow)).await;
    let id = created["id"].as_str().unwrap();
    let (_, seen) = call(
        &app,
        VIEWER,
        &org,
        "GET",
        &format!("/api/v1/flows/{id}"),
        None,
    )
    .await;
    assert_eq!(seen["draft"]["steps"][0]["input"], json!({}));
    assert!(!seen.to_string().contains("typed-secret"));
    let (_, full) = call(
        &app,
        MEMBER,
        &org,
        "GET",
        &format!("/api/v1/flows/{id}"),
        None,
    )
    .await;
    assert!(
        full.to_string().contains("typed-secret"),
        "members still edit inputs"
    );
    for path in [
        "/api/v1/connectors",
        "/api/v1/connectors/core.http",
        "/api/v1/recipes",
    ] {
        assert_eq!(app.req("GET", path, None).await.0.as_u16(), 401, "{path}");
    }
    let (status, _) = call(
        &app,
        OWNER,
        &org,
        "POST",
        "/api/v1/connections/oauth/start",
        Some(json!({
        "connector": "zoho_crm", "name": "CRM", "return_to": "https://evil.example/steal"})),
    )
    .await;
    assert_eq!(status, 422, "OAuth returns only to the web app");
    app.cleanup().await;
}
