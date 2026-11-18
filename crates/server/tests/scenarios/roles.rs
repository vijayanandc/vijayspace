//! The role matrix through the real API (doc 08 §2, doc 09 §7.3): what owners, admins, members
//! and viewers may do, including "own only" deletes. Shared helpers for `team_rules`.

use serde_json::{Value, json};

use crate::harness::TestApp;

pub(crate) struct Team {
    pub org: String,
}

pub(crate) const OWNER: &str = "ann@example.com";
pub(crate) const ADMIN: &str = "ad@example.com";
pub(crate) const MEMBER: &str = "me@example.com";
pub(crate) const VIEWER: &str = "vi@example.com";

pub(crate) async fn call(
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

/// An org with one person in each role.
pub(crate) async fn team(app: &TestApp) -> Team {
    let idp = app.idp.as_ref().unwrap();
    for (email, name) in [
        (OWNER, "Ann"),
        (ADMIN, "Ad"),
        (MEMBER, "Me"),
        (VIEWER, "Vi"),
    ] {
        idp.issue(email, email, name);
    }
    let org = app
        .req_as(
            OWNER,
            None,
            "POST",
            "/api/v1/orgs",
            Some(json!({"name": "Team"})),
        )
        .await
        .1["id"]
        .as_str()
        .unwrap()
        .to_owned();
    for (email, role) in [(ADMIN, "admin"), (MEMBER, "member"), (VIEWER, "viewer")] {
        let (_, invited) = call(
            app,
            OWNER,
            &org,
            "POST",
            "/api/v1/invitations",
            Some(json!({"email": email, "role": role})),
        )
        .await;
        let id = invited["invitation"]["id"].as_str().unwrap();
        let (status, _) = app
            .req_as(
                email,
                None,
                "POST",
                &format!("/api/v1/me/invitations/{id}/accept"),
                None,
            )
            .await;
        assert!(status.is_success());
    }
    Team { org }
}

pub(crate) async fn user_id(app: &TestApp, org: &str, email: &str) -> String {
    let (_, members) = call(app, OWNER, org, "GET", "/api/v1/members", None).await;
    members
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["user"]["email"] == email)
        .map(|m| m["user"]["id"].as_str().unwrap().to_owned())
        .unwrap()
}

pub(crate) async fn new_flow(app: &TestApp, who: &str, org: &str) -> (u16, Value) {
    let body = json!({"name": "F", "definition": {"trigger": {"type": "webhook"}, "steps": [{"key": "s", "type": "stop"}]}});
    call(app, who, org, "POST", "/api/v1/flows", Some(body)).await
}

#[tokio::test]
async fn each_role_can_do_exactly_what_the_matrix_allows() {
    let app = TestApp::spawn_signed_in().await;
    let Team { org } = team(&app).await;
    let (status, owners_flow) = new_flow(&app, OWNER, &org).await;
    assert_eq!(status, 201);
    let owners_flow = owners_flow["id"].as_str().unwrap().to_owned();

    // Viewers read, never write.
    assert_eq!(
        call(&app, VIEWER, &org, "GET", "/api/v1/flows", None)
            .await
            .0,
        200
    );
    let (status, body) = new_flow(&app, VIEWER, &org).await;
    assert_eq!(
        (status, body["code"].as_str()),
        (403, Some("auth.forbidden"))
    );
    assert_eq!(
        call(
            &app,
            VIEWER,
            &org,
            "POST",
            &format!("/api/v1/flows/{owners_flow}/publish"),
            None
        )
        .await
        .0,
        403
    );

    // Members build flows and delete only their own.
    let (status, mine) = new_flow(&app, MEMBER, &org).await;
    assert_eq!(status, 201);
    let mine = mine["id"].as_str().unwrap();
    assert_eq!(
        call(
            &app,
            MEMBER,
            &org,
            "DELETE",
            &format!("/api/v1/flows/{owners_flow}"),
            None
        )
        .await
        .0,
        403
    );
    assert_eq!(
        call(
            &app,
            MEMBER,
            &org,
            "DELETE",
            &format!("/api/v1/flows/{mine}"),
            None
        )
        .await
        .0,
        204
    );
    assert_eq!(
        call(
            &app,
            MEMBER,
            &org,
            "POST",
            "/api/v1/invitations",
            Some(json!({"email": "x@example.com", "role": "viewer"}))
        )
        .await
        .0,
        403
    );
    assert_eq!(
        call(
            &app,
            MEMBER,
            &org,
            "PATCH",
            "/api/v1/org",
            Some(json!({"retention_days": 7}))
        )
        .await
        .0,
        403
    );

    // Admins manage members (but never owners) and may delete anyone's flow; org settings are the
    // owner's.
    assert_eq!(
        call(
            &app,
            ADMIN,
            &org,
            "POST",
            "/api/v1/invitations",
            Some(json!({"email": "x@example.com", "role": "member"}))
        )
        .await
        .0,
        201
    );
    assert_eq!(
        call(
            &app,
            ADMIN,
            &org,
            "POST",
            "/api/v1/invitations",
            Some(json!({"email": "y@example.com", "role": "owner"}))
        )
        .await
        .0,
        403
    );
    let owner_id = user_id(&app, &org, OWNER).await;
    let member_id = user_id(&app, &org, MEMBER).await;
    assert_eq!(
        call(
            &app,
            ADMIN,
            &org,
            "PATCH",
            &format!("/api/v1/members/{owner_id}"),
            Some(json!({"role": "admin"}))
        )
        .await
        .0,
        403
    );
    assert_eq!(
        call(
            &app,
            ADMIN,
            &org,
            "DELETE",
            &format!("/api/v1/members/{owner_id}"),
            None
        )
        .await
        .0,
        403
    );
    let (status, changed) = call(
        &app,
        ADMIN,
        &org,
        "PATCH",
        &format!("/api/v1/members/{member_id}"),
        Some(json!({"role": "viewer"})),
    )
    .await;
    assert_eq!((status, changed["role"].as_str()), (200, Some("viewer")));
    assert_eq!(
        call(
            &app,
            ADMIN,
            &org,
            "PATCH",
            "/api/v1/org",
            Some(json!({"retention_days": 7}))
        )
        .await
        .0,
        403
    );
    assert_eq!(
        call(
            &app,
            ADMIN,
            &org,
            "DELETE",
            &format!("/api/v1/flows/{owners_flow}"),
            None
        )
        .await
        .0,
        204
    );
    let (status, org_json) = call(
        &app,
        OWNER,
        &org,
        "PATCH",
        "/api/v1/org",
        Some(json!({"retention_days": 7, "name": "Team A"})),
    )
    .await;
    assert_eq!(
        (
            status,
            org_json["retention_days"].as_u64(),
            org_json["name"].as_str()
        ),
        (200, Some(7), Some("Team A"))
    );
    app.cleanup().await;
}
