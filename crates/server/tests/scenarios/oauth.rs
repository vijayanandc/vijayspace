//! OAuth sign-ins to apps (doc 13 §6 review): the provider's answer is parked until the person who
//! started the sign-in finishes it, signed in, in the same org. A link opened by someone else, a
//! replayed callback or another member's connection cannot be used to connect an account.

use serde_json::{Value, json};

use crate::{
    harness::TestApp,
    roles::{ADMIN, MEMBER, OWNER, Team, call, team},
};

const RETURN_TO: &str = "http://app.weft.test/connections/callback";

/// The value of `key` in a URL's query (the test tokens need no decoding).
fn param(url: &str, key: &str) -> String {
    let query = url.split_once('?').map_or("", |(_, q)| q);
    query
        .split('&')
        .find_map(|pair| pair.strip_prefix(&format!("{key}=")))
        .unwrap_or_else(|| panic!("no {key} in {url}"))
        .to_owned()
}

/// Starts an `oauthapp` sign-in as `who` and returns its state.
async fn start(app: &TestApp, who: &str, org: &str, connection_id: Option<&str>) -> String {
    let (status, body) = call(
        app,
        who,
        org,
        "POST",
        "/api/v1/connections/oauth/start",
        Some(
            json!({"connector": "oauthapp", "name": "Mail", "return_to": RETURN_TO,
                    "connection_id": connection_id}),
        ),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    param(body["authorize_url"].as_str().unwrap(), "state")
}

/// The provider sends the browser back; returns the finish token Weft hands that browser.
async fn come_back(app: &TestApp, state: &str) -> String {
    let location = app
        .redirect(&format!("/oauth/callback?state={state}&code=code-1"))
        .await;
    assert!(
        location.starts_with(&format!("{RETURN_TO}?status=finish&state={state}&finish=")),
        "{location}"
    );
    param(&location, "finish")
}

async fn finish(app: &TestApp, who: &str, org: &str, state: &str, token: &str) -> (u16, Value) {
    call(
        app,
        who,
        org,
        "POST",
        "/api/v1/connections/oauth/finish",
        Some(json!({"state": state, "finish": token})),
    )
    .await
}

async fn token_exchanges(app: &TestApp) -> usize {
    app.oauth_provider
        .received_requests()
        .await
        .unwrap_or_default()
        .len()
}

#[tokio::test]
async fn only_the_person_who_started_an_oauth_sign_in_can_finish_it() {
    let app = TestApp::spawn_signed_in().await;
    let Team { org } = team(&app).await;
    // An admin opens the link a member started: the callback parks the answer, and finishing it as
    // the admin (or with a wrong finish token) is refused without spending it.
    let state = start(&app, MEMBER, &org, None).await;
    let token = come_back(&app, &state).await;
    let (status, body) = finish(&app, ADMIN, &org, &state, &token).await;
    assert_eq!(
        (status, body["code"].as_str()),
        (422, Some("oauth.state_invalid")),
        "{body}"
    );
    let (status, _) = finish(&app, MEMBER, &org, &state, "guess").await;
    assert_eq!(status, 422, "a wrong finish token");
    assert_eq!(token_exchanges(&app).await, 0, "no code was exchanged");
    // The member finishes their own; a sign-in whose callback has not come back, or a replayed
    // callback, does nothing.
    let waiting = start(&app, MEMBER, &org, None).await;
    let (status, _) = finish(&app, MEMBER, &org, &waiting, "guess").await;
    assert_eq!(status, 422, "the callback has not come back yet");
    let (status, body) = finish(&app, MEMBER, &org, &state, &token).await;
    assert_eq!(status, 200, "{body}");
    let id = body["connection_id"].as_str().unwrap().to_owned();
    let (status, replay) = app
        .req(
            "GET",
            &format!("/oauth/callback?state={state}&code=code-2"),
            None,
        )
        .await;
    assert_eq!(status.as_u16(), 400, "{replay}");
    let exchanges = app.oauth_provider.received_requests().await.unwrap();
    assert_eq!(exchanges.len(), 1);
    let form = String::from_utf8_lossy(&exchanges[0].body).into_owned();
    assert!(
        form.contains("code=code-1") && form.contains("code_verifier="),
        "{form}"
    );
    let (_, connection) = call(
        &app,
        MEMBER,
        &org,
        "GET",
        &format!("/api/v1/connections/{id}"),
        None,
    )
    .await;
    assert_eq!(connection["status"], "active", "{connection}");
    let (_, me) = app.req_as(MEMBER, None, "GET", "/api/v1/me", None).await;
    assert_eq!(connection["created_by"], me["user"]["id"]);
    app.cleanup().await;
}

#[tokio::test]
async fn an_oauth_sign_in_cannot_be_finished_in_another_org_and_errors_go_back_to_the_app() {
    let app = TestApp::spawn_signed_in().await;
    let Team { org } = team(&app).await;
    let (_, other) = app
        .req_as(
            MEMBER,
            None,
            "POST",
            "/api/v1/orgs",
            Some(json!({"name": "Mine"})),
        )
        .await;
    let other = other["id"].as_str().unwrap().to_owned();
    let state = start(&app, MEMBER, &org, None).await;
    let token = come_back(&app, &state).await;
    let (status, _) = finish(&app, MEMBER, &other, &state, &token).await;
    assert_eq!(status, 422, "started in the team org, not in their own");
    let state = start(&app, MEMBER, &org, None).await;
    let location = app
        .redirect(&format!(
            "/oauth/callback?state={state}&error=access_denied"
        ))
        .await;
    assert_eq!(
        location,
        format!("{RETURN_TO}?status=error&message=access_denied")
    );
    assert_eq!(token_exchanges(&app).await, 0);
    app.cleanup().await;
}

#[tokio::test]
async fn members_cannot_sign_another_members_oauth_connection_in_again() {
    let app = TestApp::spawn_signed_in().await;
    let Team { org } = team(&app).await;
    let state = start(&app, OWNER, &org, None).await;
    let token = come_back(&app, &state).await;
    let (_, body) = finish(&app, OWNER, &org, &state, &token).await;
    let id = body["connection_id"].as_str().unwrap().to_owned();
    let (status, body) = call(
        &app,
        MEMBER,
        &org,
        "POST",
        "/api/v1/connections/oauth/start",
        Some(
            json!({"connector": "oauthapp", "name": "Mail", "return_to": RETURN_TO,
                    "connection_id": id}),
        ),
    )
    .await;
    assert_eq!(
        (status, body["code"].as_str()),
        (403, Some("auth.forbidden"))
    );
    start(&app, ADMIN, &org, Some(&id)).await;
    let (status, body) = call(
        &app,
        OWNER,
        &org,
        "PATCH",
        &format!("/api/v1/connections/{id}"),
        Some(json!({"fields": {"region": "eu"}})),
    )
    .await;
    assert_eq!(
        status, 422,
        "settings change only by signing in again: {body}"
    );
    assert!(
        body["detail"].as_str().unwrap().contains("Reconnect"),
        "{body}"
    );
    app.cleanup().await;
}
