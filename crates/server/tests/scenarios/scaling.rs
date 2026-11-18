//! Several servers on one database: heartbeats and the staff-only status (servers and queue lag), and an
//! OAuth refresh that another server is already doing (it is awaited, not repeated).

use chrono::Duration;
use serde_json::json;
use weft_app::{
    connections::handle::{DbConnectionHandle, encrypt, refresh_lease_key},
    instances::{self, Me},
    ports::{Clock as _, NewJob, StoredConnection},
};
use weft_domain::ids::{JobId, OrgId};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

use crate::harness::TestApp;

fn server(app: &TestApp, kind: &str) -> Me {
    Me {
        id: app.deps.ids.uuid(),
        kind: kind.into(),
        host: format!("{kind}-host"),
        version: "test".into(),
        started_at: app.clock.now(),
    }
}

#[tokio::test]
async fn staff_see_live_servers_and_queue_lag_and_nobody_else_does() {
    let app = TestApp::spawn_signed_in().await;
    let idp = app.idp.as_ref().unwrap();
    idp.issue("owner", "owner@weft.test", "Owner");
    let me = app.req_as("owner", None, "GET", "/api/v1/me", None).await.1;
    let staff_org = me["orgs"][0]["id"].as_str().unwrap().to_owned();

    let api = server(&app, "api");
    let workers = [server(&app, "worker"), server(&app, "worker")];
    instances::heartbeat(&app.deps, &api).await.unwrap();
    instances::heartbeat(&app.deps, &workers[0]).await.unwrap();
    assert_eq!(
        instances::heartbeat(&app.deps, &workers[1]).await.unwrap(),
        2,
        "two servers run jobs (the API-only one does not share connection concurrency)"
    );
    assert_eq!(app.deps.instances.count(), 2);
    let now = app.clock.now();
    app.deps
        .store
        .enqueue(
            &NewJob {
                id: JobId::from_uuid(app.deps.ids.uuid()),
                queue: "executions".into(),
                kind: "run_execution".into(),
                org_id: None,
                payload: json!({}),
                run_at: now - Duration::seconds(30),
                dedupe_key: None,
            },
            now,
        )
        .await
        .unwrap();

    let (status, ops) = app
        .req_as("owner", Some(&staff_org), "GET", "/api/v1/ops/status", None)
        .await;
    assert_eq!(status.as_u16(), 200, "{ops}");
    assert_eq!(ops["instances"].as_array().map(Vec::len), Some(3), "{ops}");
    assert_eq!(
        ops["queues"],
        json!([{"queue": "executions", "ready": 1, "lag_seconds": 30}])
    );

    // A server that missed three heartbeats no longer counts; one that left is gone at once.
    app.clock.advance(Duration::seconds(46));
    assert_eq!(
        instances::heartbeat(&app.deps, &workers[0]).await.unwrap(),
        1
    );
    instances::leave(&app.deps, &workers[0]).await;
    let ops = app
        .req_as("owner", Some(&staff_org), "GET", "/api/v1/ops/status", None)
        .await
        .1;
    assert_eq!(ops["instances"], json!([]), "{ops}");

    // The owner of a customer org is not staff.
    idp.issue("eve", "eve@example.com", "Eve");
    let (_, org) = app
        .req_as(
            "eve",
            None,
            "POST",
            "/api/v1/orgs",
            Some(json!({"name": "Eve's org"})),
        )
        .await;
    let (status, _) = app
        .req_as("eve", org["id"].as_str(), "GET", "/api/v1/ops/status", None)
        .await;
    assert_eq!(status.as_u16(), 403);
    app.cleanup().await;
}

#[tokio::test]
async fn a_token_refresh_running_on_another_server_is_awaited_and_its_token_used() {
    let app = TestApp::spawn().await;
    let accounts = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth/v2/token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"access_token": "ours", "expires_in": 3600})),
        )
        .expect(0)
        .mount(&accounts)
        .await;
    app.ok("GET", "/api/v1/org", None).await;
    let now = app.clock.now();
    let org = OrgId::from_uuid(weft_server::wiring::DEFAULT_ORG);
    let connection = weft_domain::model::Connection {
        id: app.deps.new_connection_id(),
        org_id: org,
        connector: "zoho_crm".into(),
        name: "CRM".into(),
        config: json!({"accounts_server": accounts.uri()}),
        status: weft_domain::model::ConnectionStatus::Active,
        created_at: now,
        updated_at: now,
        created_by: None,
    };
    let sealed = |token: &str| {
        let secrets = json!({"access_token": token, "refresh_token": "r1",
            "expires_at": (now + Duration::hours(1)).to_rfc3339()});
        let (ciphertext, key_id) =
            encrypt(&app.deps, &connection, secrets.as_object().unwrap()).unwrap();
        StoredConnection {
            connection: connection.clone(),
            ciphertext,
            key_id,
        }
    };
    let stored = sealed("old");
    app.deps.store.insert_connection(&stored).await.unwrap();

    // Another server is refreshing this connection (it holds the refresh lease).
    let key = refresh_lease_key(connection.id);
    assert!(
        app.deps
            .store
            .acquire_lease(org, &key, "other-server", now + Duration::seconds(60), now)
            .await
            .unwrap()
    );
    // A step here got a 401 with the old token at the same time.
    let handle = DbConnectionHandle::new(app.deps.clone(), stored);
    let ours =
        tokio::spawn(
            async move { weft_connector_sdk::ConnectionHandle::force_refresh(&handle).await },
        );
    // The other server stores its new token and lets the lease go.
    app.deps
        .store
        .update_connection(&sealed("theirs"))
        .await
        .unwrap();
    app.deps
        .store
        .release_lease(org, &key, "other-server")
        .await
        .unwrap();
    assert_eq!(ours.await.unwrap().unwrap(), "theirs");
    accounts.verify().await;
    app.cleanup().await;
}
