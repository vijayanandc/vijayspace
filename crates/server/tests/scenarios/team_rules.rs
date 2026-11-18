//! Membership rules through the real API: an org always keeps an owner, anyone may leave, and
//! viewers see runs without their data (doc 08 §2).

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
    app.cleanup().await;
}
