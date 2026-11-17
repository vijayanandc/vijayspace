//! `autodesk_forma` against a mock APS server.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{collections::BTreeMap, sync::Arc, sync::atomic::Ordering};

use serde_json::{Value, json};
use weft_connector_sdk::{
    ActionCtx, ConnectionHandle, Connector, ConnectorError, HttpSender, InboundRequest,
    SubscribeRequest, SubscriptionInfo,
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

fn inbound(body: Value) -> InboundRequest {
    InboundRequest {
        method: "POST".into(),
        headers: BTreeMap::new(),
        query: BTreeMap::new(),
        body,
    }
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

fn sub_request() -> SubscribeRequest {
    SubscribeRequest {
        target_url: "https://weft.example.com/hooks/app/tok123".into(),
        verify_token: "vt".into(),
        channel_id: 1,
    }
}

#[tokio::test]
async fn subscribe_creates_one_hook_per_event_and_unsubscribe_removes_them() {
    let server = MockServer::start().await;
    let base = "/webhooks/v1/systems/autodesk.construction.issues/events";
    for (event, id) in [
        ("issue.created-1.0", "hook-1"),
        ("issue.updated-1.0", "hook-2"),
    ] {
        Mock::given(method("POST"))
            .and(path(format!("{base}/{event}/hooks")))
            .and(header("x-ads-region", "US"))
            .and(body_partial_json(json!({
                "callbackUrl": "https://weft.example.com/hooks/app/tok123",
                "scope": {"project": "b.p1"},
                "hookAttribute": {"verify_token": "vt"},
                "autoReactivateHook": true
            })))
            .respond_with(ResponseTemplate::new(201).insert_header(
                "Location",
                format!("{}{base}/{event}/hooks/{id}", server.uri()),
            ))
            .expect(1)
            .mount(&server)
            .await;
    }
    Mock::given(method("DELETE"))
        .and(path(format!("{base}/issue.created-1.0/hooks/hook-1")))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path(format!("{base}/issue.updated-1.0/hooks/hook-2")))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;
    let (c, _) = setup(&server);
    let forma = FormaConnector::new();
    let sub = forma
        .subscribe(
            &c,
            "issue_event",
            &json!({"project_id": "p1"}),
            &sub_request(),
        )
        .await
        .unwrap();
    assert_eq!(
        sub.external_id,
        "issue.created-1.0|hook-1,issue.updated-1.0|hook-2"
    );
    assert_eq!(sub.expires_at, None);
    let renewed = forma
        .renew(&c, "issue_event", &json!({}), &sub, &sub_request())
        .await
        .unwrap();
    assert_eq!(renewed, sub);
    forma.unsubscribe(&c, "issue_event", &sub).await.unwrap();
}

#[tokio::test]
async fn failed_subscription_cleans_up_and_reports_permission_problems() {
    let server = MockServer::start().await;
    let base = "/webhooks/v1/systems/autodesk.construction.issues/events";
    Mock::given(method("POST"))
        .and(path(format!("{base}/issue.created-1.0/hooks")))
        .respond_with(
            ResponseTemplate::new(201)
                .insert_header("Location", format!("{base}/issue.created-1.0/hooks/hook-1")),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("{base}/issue.deleted-1.0/hooks")))
        .respond_with(ResponseTemplate::new(403).set_body_string("not admin"))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path(format!("{base}/issue.created-1.0/hooks/hook-1")))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let (c, _) = setup(&server);
    let input = json!({"project_id": "p1", "events": ["issue.created-1.0", "issue.deleted-1.0"]});
    let err = FormaConnector::new()
        .subscribe(&c, "issue_event", &input, &sub_request())
        .await
        .unwrap_err();
    assert!(
        matches!(&err, ConnectorError::Permanent { status: Some(403), message } if message.contains("autodesk.permission")),
        "{err:?}"
    );
}

fn callback(token: &str, payload: Value) -> Value {
    json!({
        "version": "1.0",
        "resourceUrn": "urn:adsk.wipprod:dm.issue:iss-1",
        "hook": {"hookId": "hook-1", "event": "issue.updated-1.0", "system": "autodesk.construction.issues",
                 "scope": {"project": "b.p1"}, "hookAttribute": {"verify_token": token}},
        "payload": payload
    })
}

#[tokio::test]
async fn events_are_verified_and_the_issue_is_fetched() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/construction/issues/v1/projects/p1/issues/iss-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": "iss-1", "title": "Leak", "status": "open", "updatedAt": "2026-01-02T00:00:00.000Z"})))
        .expect(1)
        .mount(&server)
        .await;
    let (c, _) = setup(&server);
    let forma = FormaConnector::new();
    let input = json!({"project_id": "p1"});
    let bad = forma
        .handle_event(
            &c,
            "issue_event",
            &input,
            &inbound(callback("wrong", json!({"id": "iss-1"}))),
            "vt",
        )
        .await
        .unwrap_err();
    assert!(matches!(bad, ConnectorError::InvalidInput { .. }));
    let ping = forma
        .handle_event(
            &c,
            "issue_event",
            &input,
            &inbound(json!({"hook": {}})),
            "vt",
        )
        .await
        .unwrap();
    assert!(ping.items.is_empty());
    let batch = forma
        .handle_event(
            &c,
            "issue_event",
            &input,
            &inbound(callback("vt", json!({"id": "iss-1"}))),
            "vt",
        )
        .await
        .unwrap();
    let item = &batch.items[0];
    assert_eq!(item.dedupe_key, "iss-1:2026-01-02T00:00:00.000Z");
    assert_eq!(item.record_id.as_deref(), Some("iss-1"));
    assert_eq!(item.data["project_id"], "p1");
    assert_eq!(item.data["event"], "issue.updated-1.0");
    assert_eq!(item.data["issue"]["title"], "Leak");
}

#[tokio::test]
async fn cost_events_pass_the_payload_through() {
    let server = MockServer::start().await;
    let (c, _) = setup(&server);
    let body = json!({
        "hook": {"event": "changeOrder.updated-1.0", "system": "autodesk.construction.cost", "scope": {"project": "b.p1"}, "hookAttribute": {"verify_token": "vt"}},
        "payload": {"id": "co-1", "status": "approved", "amount": 12_500, "updatedAt": "2026-01-03T00:00:00Z"}
    });
    let batch = FormaConnector::new()
        .handle_event(&c, "cost_event", &json!({}), &inbound(body), "vt")
        .await
        .unwrap();
    let item = &batch.items[0];
    assert_eq!(
        item.dedupe_key,
        "co-1:changeOrder.updated-1.0:2026-01-03T00:00:00Z"
    );
    assert_eq!(item.data["payload"]["amount"], 12_500);
    assert_eq!(item.data["project_id"], "p1");
}

#[tokio::test]
async fn polling_records_a_baseline_then_returns_changes_oldest_first() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/construction/issues/v1/projects/p1/issues"))
        .and(query_param(
            "filter[updatedAt]",
            "2026-01-01T00:00:00.000Z..",
        ))
        .and(query_param("sortBy", "updatedAt"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"results": [
            {"id": "b", "updatedAt": "2026-01-03T00:00:00.000Z"},
            {"id": "a", "updatedAt": "2026-01-02T00:00:00.000Z"}
        ]})))
        .expect(1)
        .mount(&server)
        .await;
    let (c, _) = setup(&server);
    let forma = FormaConnector::new();
    let input = json!({"project_id": "p1"});
    let baseline = forma.poll(&c, "issue_event", &input, None).await.unwrap();
    assert!(baseline.items.is_empty());
    let cursor = baseline.cursor.unwrap();
    assert_eq!(cursor, json!("2026-01-01T00:00:00.000Z"));
    let changed = forma
        .poll(&c, "issue_event", &input, Some(&cursor))
        .await
        .unwrap();
    let keys: Vec<&str> = changed
        .items
        .iter()
        .map(|i| i.dedupe_key.as_str())
        .collect();
    assert_eq!(
        keys,
        vec!["a:2026-01-02T00:00:00.000Z", "b:2026-01-03T00:00:00.000Z"]
    );
    assert_eq!(changed.cursor, Some(json!("2026-01-03T00:00:00.000Z")));
    let unsupported = forma
        .poll(&c, "cost_event", &input, None)
        .await
        .unwrap_err();
    assert!(matches!(unsupported, ConnectorError::Unsupported { .. }));
}

#[tokio::test]
async fn subscription_info_round_trips_for_unsubscribe_without_data() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/webhooks/v1/systems/autodesk.construction.cost/events/changeOrder.created-1.0/hooks/h9"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let (c, _) = setup(&server);
    let sub = SubscriptionInfo {
        external_id: "changeOrder.created-1.0|h9".into(),
        expires_at: None,
        data: Value::Null,
    };
    FormaConnector::new()
        .unsubscribe(&c, "cost_event", &sub)
        .await
        .unwrap();
}
