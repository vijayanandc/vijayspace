//! `zoho_crm` triggers against a mock Zoho CRM API: notification channels, events, polling.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{collections::BTreeMap, sync::Arc};

use serde_json::{Value, json};
use weft_connector_sdk::{
    ActionCtx, Connector, InboundRequest, SubscribeRequest,
    testing::{StaticConnection, ctx},
};
use weft_connectors::zoho_crm::ZohoCrmConnector;
use weft_http::{ReqwestSender, SsrfPolicy};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path, query_param},
};

fn zoho_ctx(server: &MockServer) -> (ActionCtx, Arc<StaticConnection>) {
    let conn = StaticConnection::new(
        json!({"api_domain": server.uri()}),
        json!({"access_token": "tok"}),
    );
    let cx = ctx(
        Arc::new(ReqwestSender::new(SsrfPolicy::allow_private()).unwrap()),
        Some(conn.clone()),
    );
    (cx, conn)
}

async fn mount_fields(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/crm/v8/settings/fields"))
        .and(query_param("module", "Deals"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"fields": [
            {"api_name": "Deal_Name", "field_label": "Deal Name", "data_type": "text", "system_mandatory": true},
            {"api_name": "Stage", "field_label": "Stage", "data_type": "picklist", "pick_list_values": [{"display_value": "Closed Won", "actual_value": "Closed Won"}]},
            {"api_name": "Amount", "field_label": "Amount", "data_type": "currency"}
        ]})))
        .mount(server)
        .await;
}

fn subscribe_req() -> SubscribeRequest {
    SubscribeRequest {
        target_url: "https://weft.example.com/hooks/app/abc".into(),
        verify_token: "tok123".into(),
        channel_id: 42,
    }
}

#[tokio::test]
async fn subscribe_and_renew_notification_channels() {
    let server = MockServer::start().await;
    let watch_ok = json!({"watch": [{"code": "SUCCESS", "status": "success", "details": {"events": [{"channel_expiry": "2026-01-07T23:50:00+00:00", "channel_id": "42", "resource_name": "Deals"}]}}]});
    Mock::given(method("POST"))
        .and(path("/crm/v8/actions/watch"))
        .respond_with(ResponseTemplate::new(200).set_body_json(watch_ok.clone()))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path("/crm/v8/actions/watch"))
        .respond_with(ResponseTemplate::new(200).set_body_json(watch_ok))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/crm/v8/actions/watch"))
        .and(query_param("channel_ids", "42"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"watch": [{"code": "SUCCESS", "status": "success"}]})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let (cx, _) = zoho_ctx(&server);
    let c = ZohoCrmConnector::new();
    let input = json!({"module": "Deals", "events": ["create", "edit"]});
    let sub = c
        .subscribe(&cx, "record_event", &input, &subscribe_req())
        .await
        .unwrap();
    assert_eq!(sub.external_id, "42");
    assert_eq!(sub.expires_at.as_deref(), Some("2026-01-07T23:50:00+00:00"));
    let sent: Value =
        serde_json::from_slice(&server.received_requests().await.unwrap()[0].body).unwrap();
    assert_eq!(
        sent["watch"][0]["events"],
        json!(["Deals.create", "Deals.edit"])
    );
    assert_eq!(
        sent["watch"][0]["notify_url"],
        "https://weft.example.com/hooks/app/abc"
    );
    assert_eq!(sent["watch"][0]["token"], "tok123");
    assert_eq!(
        sent["watch"][0]["channel_expiry"],
        "2026-01-07T23:50:00+00:00"
    );
    c.renew(&cx, "record_event", &input, &sub, &subscribe_req())
        .await
        .unwrap();
    c.unsubscribe(&cx, "record_event", &sub).await.unwrap();
}

fn notification(body: Value) -> InboundRequest {
    InboundRequest {
        method: "POST".into(),
        headers: BTreeMap::new(),
        query: BTreeMap::new(),
        body,
    }
}

#[tokio::test]
async fn notifications_are_verified_then_records_fetched() {
    let server = MockServer::start().await;
    mount_fields(&server).await;
    Mock::given(method("GET")).and(path("/crm/v8/Deals")).and(query_param("ids", "4876"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": [{"id": "4876", "Stage": "Closed Won", "Modified_Time": "2026-01-01T10:00:00+05:30"}]})))
        .expect(1)
        .mount(&server).await;
    let (cx, _) = zoho_ctx(&server);
    let c = ZohoCrmConnector::new();
    let input = json!({"module": "Deals", "events": ["edit", "delete"]});
    // Zoho lists the changed fields per record.
    let body = json!({"module": "Deals", "operation": "update", "ids": ["4876"], "token": "tok123", "channel_id": "42", "affected_fields": [{"4876": ["Stage"]}]});
    let mut forged = body.clone();
    forged["token"] = json!("wrong");
    assert!(
        c.handle_event(&cx, "record_event", &input, &notification(forged), "tok123")
            .await
            .is_err()
    );
    let batch = c
        .handle_event(&cx, "record_event", &input, &notification(body), "tok123")
        .await
        .unwrap();
    assert_eq!(batch.items[0].dedupe_key, "4876:2026-01-01T10:00:00+05:30");
    assert_eq!(batch.items[0].data["record"]["Stage"], "Closed Won");
    assert_eq!(batch.items[0].data["operation"], "edit");
    assert_eq!(batch.items[0].data["affected_fields"], json!(["Stage"]));
    let deleted = c
        .handle_event(
            &cx,
            "record_event",
            &input,
            &notification(
                json!({"module": "Deals", "operation": "delete", "ids": ["5"], "token": "tok123"}),
            ),
            "tok123",
        )
        .await
        .unwrap();
    assert_eq!(deleted.items[0].dedupe_key, "5:deleted");
    let created = c
        .handle_event(
            &cx,
            "record_event",
            &input,
            &notification(
                json!({"module": "Deals", "operation": "insert", "ids": ["6"], "token": "tok123"}),
            ),
            "tok123",
        )
        .await
        .unwrap();
    assert!(
        created.items.is_empty(),
        "create events were not subscribed"
    );
}

#[tokio::test]
async fn polling_baseline_then_modified_records() {
    let server = MockServer::start().await;
    mount_fields(&server).await;
    Mock::given(method("GET"))
        .and(path("/crm/v8/Deals"))
        .and(query_param("sort_by", "Modified_Time"))
        .and(query_param("sort_order", "asc"))
        .and(header("if-modified-since", "2026-01-01T00:00:00+00:00"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": [
            {"id": "1", "Created_Time": "2026-01-01T05:31:00+05:30", "Modified_Time": "2026-01-01T05:31:00+05:30"},
            {"id": "2", "Created_Time": "2025-12-01T09:00:00+05:30", "Modified_Time": "2026-01-01T05:32:00+05:30"}
        ]})))
        .mount(&server)
        .await;
    // Nothing modified since the last record: Zoho answers 304.
    Mock::given(method("GET"))
        .and(path("/crm/v8/Deals"))
        .and(header("if-modified-since", "2026-01-01T05:32:00+05:30"))
        .respond_with(ResponseTemplate::new(304))
        .mount(&server)
        .await;
    let (cx, _) = zoho_ctx(&server);
    let c = ZohoCrmConnector::new();
    let input = json!({"module": "Deals"});
    let baseline = c.poll(&cx, "record_event", &input, None).await.unwrap();
    assert!(baseline.items.is_empty());
    assert_eq!(baseline.cursor, Some(json!("2026-01-01T00:00:00+00:00")));
    let next = c
        .poll(&cx, "record_event", &input, baseline.cursor.as_ref())
        .await
        .unwrap();
    assert_eq!(next.items.len(), 2);
    assert_eq!(
        (
            next.items[0].data["operation"].clone(),
            next.items[1].data["operation"].clone()
        ),
        (json!("create"), json!("edit")),
        "created equals modified: a new record"
    );
    assert_eq!(next.cursor, Some(json!("2026-01-01T05:32:00+05:30")));
    let created_only = json!({"module": "Deals", "events": ["create"]});
    let only = c
        .poll(&cx, "record_event", &created_only, baseline.cursor.as_ref())
        .await
        .unwrap();
    assert_eq!(
        only.items.len(),
        1,
        "the trigger's events filter polled records too"
    );
    let idle = c
        .poll(&cx, "record_event", &input, next.cursor.as_ref())
        .await
        .unwrap();
    assert!(
        idle.items.is_empty(),
        "304 Not Modified is an empty poll, not an error"
    );
    assert_eq!(idle.cursor, next.cursor);
}

#[tokio::test]
async fn oauth_config_hooks_capture_data_centre() {
    let c = ZohoCrmConnector::new();
    let mut q = BTreeMap::new();
    q.insert(
        "accounts-server".to_owned(),
        "https://accounts.zoho.in".to_owned(),
    );
    q.insert("location".to_owned(), "in".to_owned());
    assert_eq!(
        c.oauth_callback_config(&q),
        json!({"accounts_server": "https://accounts.zoho.in", "location": "in"})
    );
    assert_eq!(
        c.oauth_token_config(
            &json!({"access_token": "a", "api_domain": "https://www.zohoapis.in"})
        ),
        json!({"api_domain": "https://www.zohoapis.in"})
    );
    let spec = c.manifest().auth.oauth2.clone().unwrap();
    assert_eq!(
        weft_connector_sdk::oauth::fill(
            &spec.token_url,
            &json!({"accounts_server": "https://accounts.zoho.in"})
        ),
        "https://accounts.zoho.in/oauth/v2/token"
    );
}
