//! `autodesk_forma` against a mock APS server.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{sync::Arc, sync::atomic::Ordering};

use serde_json::json;
use weft_connector_sdk::{
    ActionCtx, ConnectionHandle, Connector, ConnectorError, HttpSender,
    testing::{StaticConnection, ctx},
};
use weft_connectors::autodesk_forma::FormaConnector;
use weft_http::{ReqwestSender, SsrfPolicy};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_json, body_partial_json, header, method, path, query_param},
};

fn setup(server: &MockServer) -> (ActionCtx, Arc<StaticConnection>) {
    let conn = StaticConnection::new(
        json!({"base_url": server.uri(), "region": "US", "account_id": "acc1"}),
        json!({"access_token": "tok"}),
    );
    let http: Arc<dyn HttpSender> =
        Arc::new(ReqwestSender::new(SsrfPolicy::allow_private()).unwrap());
    let handle: Arc<dyn ConnectionHandle> = conn.clone();
    (ctx(http, Some(handle)), conn)
}

#[tokio::test]
async fn manifest_is_consistent() {
    let c = FormaConnector::new();
    let m = c.manifest();
    assert_eq!(m.key, "autodesk_forma");
    assert!(m.requires_connection());
    let sources = ["accounts", "projects", "issue_subtypes"];
    for field in m
        .actions
        .iter()
        .flat_map(|a| &a.fields)
        .chain(m.triggers.iter().flat_map(|t| &t.fields))
    {
        if let Some(src) = &field.options_from {
            assert!(
                sources.contains(&src.as_str()),
                "unknown option source {src}"
            );
        }
    }
    assert!(m.trigger("issue_event").unwrap().supports_polling);
}

#[tokio::test]
async fn create_project_sends_the_admin_api_shape() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/construction/admin/v1/accounts/acc1/projects"))
        .and(header("authorization", "Bearer tok"))
        .and(body_json(json!({
            "name": "Riverside Clinic", "type": "Office", "jobNumber": "J-1", "startDate": "2026-02-01",
            "projectValue": {"value": 100_000.0, "currency": "USD"}
        })))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({"id": "p1", "name": "Riverside Clinic", "status": "pending"})))
        .expect(1)
        .mount(&server)
        .await;
    let (c, _) = setup(&server);
    let input = json!({"name": "Riverside Clinic", "job_number": "J-1", "start_date": "2026-02-01T00:00:00Z", "project_value": 100_000});
    let out = FormaConnector::new()
        .execute(&c, "create_project", input)
        .await
        .unwrap();
    assert_eq!(out["id"], "p1");
    assert_eq!(out["status"], "pending");
    assert_eq!(out["account_id"], "acc1");
}

#[tokio::test]
async fn create_issue_strips_the_b_prefix_and_publishes() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/construction/issues/v1/projects/p1/issues"))
        .and(body_partial_json(json!({"title": "Leak", "issueSubtypeId": "st1", "status": "open", "published": true, "dueDate": "2026-03-01"})))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"id": "iss-1", "title": "Leak", "displayId": 7})))
        .expect(1)
        .mount(&server)
        .await;
    let (c, _) = setup(&server);
    let input = json!({"project_id": "b.p1", "title": "Leak", "issue_subtype_id": "st1", "due_date": "2026-03-01"});
    let out = FormaConnector::new()
        .execute(&c, "create_issue", input)
        .await
        .unwrap();
    assert_eq!(out["id"], "iss-1");
}

#[tokio::test]
async fn expired_token_is_refreshed_once_and_retried() {
    let server = MockServer::start().await;
    let issue_path = "/construction/issues/v1/projects/p1/issues/iss-1";
    Mock::given(method("GET"))
        .and(path(issue_path))
        .and(header("authorization", "Bearer tok"))
        .respond_with(ResponseTemplate::new(401))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(issue_path))
        .and(header("authorization", "Bearer refreshed-token"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"id": "iss-1", "status": "open"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let (c, conn) = setup(&server);
    let out = FormaConnector::new()
        .execute(
            &c,
            "get_issue",
            json!({"project_id": "p1", "issue_id": "iss-1"}),
        )
        .await
        .unwrap();
    assert_eq!(out["status"], "open");
    assert_eq!(conn.refreshes.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn update_issue_requires_a_change() {
    let server = MockServer::start().await;
    let (c, _) = setup(&server);
    let err = FormaConnector::new()
        .execute(
            &c,
            "update_issue",
            json!({"project_id": "p1", "issue_id": "i"}),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, ConnectorError::InvalidInput { .. }));
}

#[tokio::test]
async fn option_sources_load_accounts_projects_and_subtypes() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/project/v1/hubs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"data": [{"id": "b.acc1", "attributes": {"name": "Acme Builders"}}]}),
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/construction/admin/v1/accounts/acc1/projects"))
        .and(query_param("limit", "200"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"results": [{"id": "p1", "name": "Riverside Clinic"}]})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/construction/issues/v1/projects/p1/issue-types"))
        .and(query_param("include", "subtypes"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"results": [
            {"id": "t1", "title": "Quality", "subtypes": [{"id": "st1", "title": "Defect"}, {"id": "st2", "title": "Punch item"}]}
        ]})))
        .mount(&server)
        .await;
    let (c, _) = setup(&server);
    let forma = FormaConnector::new();
    let accounts = forma.options(&c, "accounts", &json!({})).await.unwrap();
    assert_eq!(
        (accounts[0].value.as_str(), accounts[0].label.as_str()),
        ("acc1", "Acme Builders")
    );
    let projects = forma.options(&c, "projects", &json!({})).await.unwrap();
    assert_eq!(projects[0].value, "p1");
    let subtypes = forma
        .options(&c, "issue_subtypes", &json!({"project_id": "b.p1"}))
        .await
        .unwrap();
    let labels: Vec<&str> = subtypes.iter().map(|o| o.label.as_str()).collect();
    assert_eq!(labels, vec!["Quality › Defect", "Quality › Punch item"]);
    let info = forma.test_connection(&c).await.unwrap();
    assert_eq!(info, json!({"hubs": 1, "accounts": ["Acme Builders"]}));
}
