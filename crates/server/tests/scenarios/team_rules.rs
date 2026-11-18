//! Membership rules through the real API: an org always keeps an owner, anyone may leave, viewers see runs
//! without their data (doc 08 §2), and only staff create organisations.

use serde_json::json;

use crate::{
    harness::TestApp,
    roles::{ADMIN, MEMBER, OWNER, Team, VIEWER, call, new_flow, team, user_id},
};

#[tokio::test]
async fn an_org_always_keeps_an_owner_and_anyone_may_leave() {
    let app = TestApp::spawn_signed_in().await;
    let Team { org } = team(&app).await;
    let owner_id = user_id(&app, &org, OWNER).await;
    let admin_id = user_id(&app, &org, ADMIN).await;
    let viewer_id = user_id(&app, &org, VIEWER).await;
    let (status, body) = call(
        &app,
        OWNER,
        &org,
        "PATCH",
        &format!("/api/v1/members/{owner_id}"),
        Some(json!({"role": "admin"})),
    )
    .await;
    assert_eq!(
        (status, body["code"].as_str()),
        (409, Some("member.last_owner"))
    );
    assert_eq!(
        call(
            &app,
            OWNER,
            &org,
            "DELETE",
            &format!("/api/v1/members/{owner_id}"),
            None
        )
        .await
        .0,
        409
    );
    assert_eq!(
        call(
            &app,
            OWNER,
            &org,
            "PATCH",
            &format!("/api/v1/members/{admin_id}"),
            Some(json!({"role": "owner"}))
        )
        .await
        .0,
        200
    );
    assert_eq!(
        call(
            &app,
            OWNER,
            &org,
            "PATCH",
            &format!("/api/v1/members/{owner_id}"),
            Some(json!({"role": "member"}))
        )
        .await
        .0,
        200
    );
    // A viewer leaves; afterwards the org is simply not there for them.
    assert_eq!(
        call(
            &app,
            VIEWER,
            &org,
            "DELETE",
            &format!("/api/v1/members/{viewer_id}"),
            None
        )
        .await
        .0,
        204
    );
    let (status, body) = call(&app, VIEWER, &org, "GET", "/api/v1/flows", None).await;
    assert_eq!(
        (status, body["code"].as_str()),
        (404, Some("org.not_found"))
    );
    app.cleanup().await;
}

#[tokio::test]
async fn viewers_see_runs_without_their_data() {
    let app = TestApp::spawn_signed_in().await;
    let Team { org } = team(&app).await;
    let (_, flow) = new_flow(&app, OWNER, &org).await;
    let id = flow["id"].as_str().unwrap();
    call(
        &app,
        OWNER,
        &org,
        "POST",
        &format!("/api/v1/flows/{id}/publish"),
        None,
    )
    .await;
    let (_, enabled) = call(
        &app,
        OWNER,
        &org,
        "POST",
        &format!("/api/v1/flows/{id}/enable"),
        None,
    )
    .await;
    let (_, viewed) = call(
        &app,
        VIEWER,
        &org,
        "GET",
        &format!("/api/v1/flows/{id}"),
        None,
    )
    .await;
    assert_eq!(
        viewed["trigger"]["webhook_url"], "",
        "viewers cannot start flows from outside"
    );
    assert_eq!(viewed["trigger"]["verify_token"], "");
    let url = enabled["trigger"]["webhook_url"].as_str().unwrap();
    let hook = &url[url.find("/hooks/").unwrap()..];
    let (status, accepted) = app
        .req("POST", hook, Some(json!({"secret": "card 4242"})))
        .await;
    assert!(status.is_success(), "webhooks need no sign-in: {accepted}");
    app.drain().await;
    let exe = accepted["executions"][0].as_str().unwrap();
    let (_, full) = call(
        &app,
        MEMBER,
        &org,
        "GET",
        &format!("/api/v1/executions/{exe}"),
        None,
    )
    .await;
    assert_eq!(full["payloads_hidden"], false);
    assert_eq!(full["trigger_data"]["body"]["secret"], "card 4242");
    let (status, seen) = call(
        &app,
        VIEWER,
        &org,
        "GET",
        &format!("/api/v1/executions/{exe}"),
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(seen["payloads_hidden"], true);
    assert!(!seen.to_string().contains("4242"), "{seen}");
    assert_eq!(seen["status"], "succeeded");
    // The detailed log: statuses for viewers too, never data.
    let (status, log) = call(
        &app,
        VIEWER,
        &org,
        "GET",
        &format!("/api/v1/executions/{exe}/log"),
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert!(!log.to_string().contains("4242"), "{log}");
    assert_eq!(
        log["entries"].as_array().unwrap().last().unwrap()["message"],
        "Run succeeded in 0 ms"
    );
    app.cleanup().await;
}

#[tokio::test]
async fn an_invitation_matches_its_exact_address_only() {
    let app = TestApp::spawn_signed_in().await;
    let Team { org } = team(&app).await;
    let (status, _) = call(
        &app,
        OWNER,
        &org,
        "POST",
        "/api/v1/invitations",
        Some(json!({"email": "alice@acme.com", "role": "admin"})),
    )
    .await;
    assert_eq!(status, 201);
    let (status, _) = call(
        &app,
        OWNER,
        &org,
        "POST",
        "/api/v1/invitations",
        Some(json!({"email": "alice@acmé.com", "role": "viewer"})),
    )
    .await;
    assert_eq!(status, 422, "look-alike (non-ASCII) addresses are refused");
    // A verified look-alike identity (an IDN domain) sees and accepts nothing.
    app.idp
        .as_ref()
        .unwrap()
        .issue("mallory", "alice@acmé.com", "Mallory");
    let me = app
        .req_as("mallory", None, "GET", "/api/v1/me", None)
        .await
        .1;
    assert_eq!(me["invitations"], json!([]), "{me}");
    let (_, open) = call(&app, OWNER, &org, "GET", "/api/v1/invitations", None).await;
    let id = open.as_array().unwrap()[0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let (status, _) = app
        .req_as(
            "mallory",
            None,
            "POST",
            &format!("/api/v1/me/invitations/{id}/accept"),
            None,
        )
        .await;
    assert_eq!(status.as_u16(), 404);
    app.cleanup().await;
}

#[tokio::test]
async fn only_staff_create_organisations_by_default() {
    let app = TestApp::spawn_staff_only().await;
    let idp = app.idp.as_ref().unwrap();
    idp.issue("eve", "eve@example.com", "Eve");
    idp.issue("staff", "owner@weft.test", "Owner");
    // Someone from another app on the shared sign-in tenant: no org, no way to make one.
    let (_, me) = app.req_as("eve", None, "GET", "/api/v1/me", None).await;
    assert_eq!(
        (me["staff"].clone(), me["can_create_orgs"].clone()),
        (json!(false), json!(false)),
        "{me}"
    );
    let (status, body) = app
        .req_as(
            "eve",
            None,
            "POST",
            "/api/v1/orgs",
            Some(json!({"name": "Eve's"})),
        )
        .await;
    assert_eq!(
        (status.as_u16(), body["code"].as_str()),
        (403, Some("auth.forbidden")),
        "{body}"
    );
    // Staff (an owner of the default org) set up the customer's org and invite its people.
    let (_, me) = app.req_as("staff", None, "GET", "/api/v1/me", None).await;
    assert_eq!(
        (me["staff"].clone(), me["can_create_orgs"].clone()),
        (json!(true), json!(true)),
        "{me}"
    );
    let (status, org) = app
        .req_as(
            "staff",
            None,
            "POST",
            "/api/v1/orgs",
            Some(json!({"name": "Acme"})),
        )
        .await;
    assert_eq!(status.as_u16(), 201, "{org}");
    let acme = org["id"].as_str().unwrap();
    let (status, _) = app
        .req_as(
            "staff",
            Some(acme),
            "POST",
            "/api/v1/invitations",
            Some(json!({"email": "eve@example.com", "role": "admin"})),
        )
        .await;
    assert_eq!(status.as_u16(), 201);
    let (_, me) = app.req_as("eve", None, "GET", "/api/v1/me", None).await;
    let invitation = me["invitations"][0]["id"].as_str().unwrap().to_owned();
    let (status, _) = app
        .req_as(
            "eve",
            None,
            "POST",
            &format!("/api/v1/me/invitations/{invitation}/accept"),
            None,
        )
        .await;
    assert_eq!(status.as_u16(), 200);
    // An admin of a customer org is not staff: still no new orgs, and no platform status.
    let (_, me) = app.req_as("eve", None, "GET", "/api/v1/me", None).await;
    assert_eq!(me["can_create_orgs"], false, "{me}");
    let (status, _) = app
        .req_as("eve", Some(acme), "GET", "/api/v1/ops/status", None)
        .await;
    assert_eq!(status.as_u16(), 403);
    // Staff keep the platform status while acting in a customer's org (the Platform page in any org).
    let (status, ops) = app
        .req_as("staff", Some(acme), "GET", "/api/v1/ops/status", None)
        .await;
    assert_eq!(status.as_u16(), 200, "{ops}");
    app.cleanup().await;
}
