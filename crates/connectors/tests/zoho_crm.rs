//! `zoho_crm` connector against a mock Zoho CRM API.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, atomic::Ordering};

use serde_json::{Value, json};
use weft_connector_sdk::{
    ActionCtx, Connector, ConnectorError, FieldKind,
    testing::{StaticConnection, ctx},
};
use weft_connectors::zoho_crm::ZohoCrmConnector;
use weft_http::{ReqwestSender, SsrfPolicy};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_json, header, method, path, query_param},
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

fn success(id: &str) -> Value {
    json!({"data": [{"code": "SUCCESS", "status": "success", "message": "record added", "details": {"id": id, "Modified_Time": "2026-01-01T10:00:00+05:30"}}]})
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

#[tokio::test]
async fn create_record_sends_oauth_header_and_suppresses_workflows() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/crm/v8/Deals"))
        .and(header("authorization", "Zoho-oauthtoken tok"))
        .and(body_json(
            json!({"data": [{"Deal_Name": "Acme", "Stage": "Qualification"}], "trigger": []}),
        ))
        .respond_with(ResponseTemplate::new(201).set_body_json(success("4876")))
        .expect(1)
        .mount(&server)
        .await;
    let (cx, _) = zoho_ctx(&server);
    let out = ZohoCrmConnector::new()
        .execute(
            &cx,
            "create_record",
            json!({"module": "Deals", "values": {"Deal_Name": "Acme", "Stage": "Qualification"}}),
        )
        .await
        .unwrap();
    assert_eq!(out["id"], "4876");
}

#[tokio::test]
async fn update_and_upsert_request_shapes() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/crm/v8/Deals/4876"))
        .and(body_json(json!({"data": [{"Stage": "Closed Won"}]})))
        .respond_with(ResponseTemplate::new(200).set_body_json(success("4876")))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/crm/v8/Contacts/upsert"))
        .and(body_json(json!({"data": [{"Email": "ann@x.io", "Last_Name": "Lee"}], "duplicate_check_fields": ["Email"], "trigger": []})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": [{"code": "SUCCESS", "status": "success", "action": "update", "duplicate_field": "Email", "details": {"id": "99"}}]})))
        .expect(1)
        .mount(&server)
        .await;
    let (cx, _) = zoho_ctx(&server);
    let c = ZohoCrmConnector::new();
    let upd = c.execute(&cx, "update_record", json!({"module": "Deals", "record_id": "4876", "values": {"Stage": "Closed Won"}, "trigger_workflows": true})).await.unwrap();
    assert_eq!(upd["id"], "4876");
    let up = c.execute(&cx, "upsert_record", json!({"module": "Contacts", "values": {"Email": "ann@x.io", "Last_Name": "Lee"}, "duplicate_check_fields": "Email"})).await.unwrap();
    assert_eq!(
        (up["id"].as_str(), up["action"].as_str()),
        (Some("99"), Some("update"))
    );
}

#[tokio::test]
async fn expired_token_is_refreshed_once_then_auth_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/crm/v8/Deals/1"))
        .and(header("authorization", "Zoho-oauthtoken tok"))
        .respond_with(
            ResponseTemplate::new(401)
                .set_body_json(json!({"code": "INVALID_TOKEN", "message": "invalid oauth token"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/crm/v8/Deals/1"))
        .and(header("authorization", "Zoho-oauthtoken refreshed-token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"data": [{"id": "1", "Deal_Name": "Acme"}]})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/crm/v8/Deals/2"))
        .respond_with(
            ResponseTemplate::new(401)
                .set_body_json(json!({"code": "INVALID_TOKEN", "message": "invalid oauth token"})),
        )
        .mount(&server)
        .await;
    let (cx, conn) = zoho_ctx(&server);
    let c = ZohoCrmConnector::new();
    let out = c
        .execute(
            &cx,
            "get_record",
            json!({"module": "Deals", "record_id": "1"}),
        )
        .await
        .unwrap();
    assert_eq!(out["record"]["Deal_Name"], "Acme");
    assert_eq!(conn.refreshes.load(Ordering::SeqCst), 1);
    let err = c
        .execute(
            &cx,
            "get_record",
            json!({"module": "Deals", "record_id": "2"}),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, ConnectorError::AuthExpired { .. }));
}

#[tokio::test]
async fn record_and_rate_limit_errors_are_mapped() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/crm/v8/Contacts"))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({"data": [{"code": "DUPLICATE_DATA", "status": "error", "message": "duplicate data", "details": {"api_name": "Email"}}]})))
        .mount(&server).await;
    Mock::given(method("POST"))
        .and(path("/crm/v8/Leads"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("Retry-After", "30")
                .set_body_json(json!({"code": "TOO_MANY_REQUESTS"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST")).and(path("/crm/v8/Deals"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({"code": "MANDATORY_NOT_FOUND", "message": "required field not found", "details": {"api_name": "Deal_Name"}, "status": "error"})))
        .mount(&server).await;
    let (cx, _) = zoho_ctx(&server);
    let c = ZohoCrmConnector::new();
    let create = |m: &'static str| {
        c.execute(
            &cx,
            "create_record",
            json!({"module": m, "values": {"x": 1}}),
        )
    };
    assert!(
        matches!(create("Contacts").await.unwrap_err(), ConnectorError::InvalidInput { field: Some(f), .. } if f == "Email")
    );
    assert_eq!(
        create("Leads").await.unwrap_err().retry_after_secs(),
        Some(30)
    );
    assert!(
        matches!(create("Deals").await.unwrap_err(), ConnectorError::InvalidInput { field: Some(f), .. } if f == "Deal_Name")
    );
    // A single-record write that fails with 400 carries its error in `data[0]` (Zoho v8).
    Mock::given(method("POST")).and(path("/crm/v8/Accounts"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({"data": [{"code": "INVALID_DATA", "status": "error", "message": "invalid data", "details": {"api_name": "Annual_Revenue"}}]})))
        .mount(&server).await;
    assert!(
        matches!(create("Accounts").await.unwrap_err(), ConnectorError::InvalidInput { field: Some(f), .. } if f == "Annual_Revenue")
    );
}

#[tokio::test]
async fn a_missing_permission_asks_for_a_reconnect_without_a_pointless_refresh() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/crm/v8/org"))
        .respond_with(ResponseTemplate::new(401).set_body_json(
            json!({"code": "OAUTH_SCOPE_MISMATCH", "message": "invalid oauth scope to access this URL", "status": "error"}),
        ))
        .mount(&server)
        .await;
    let (cx, conn) = zoho_ctx(&server);
    let err = ZohoCrmConnector::new()
        .test_connection(&cx)
        .await
        .unwrap_err();
    assert!(matches!(err, ConnectorError::AuthExpired { .. }), "{err:?}");
    assert!(err.to_string().contains("lacks a permission"), "{err}");
    assert_eq!(conn.refreshes.load(Ordering::SeqCst), 0);
    let scopes = &ZohoCrmConnector::new()
        .manifest()
        .auth
        .oauth2
        .clone()
        .unwrap()
        .scopes;
    assert!(
        scopes.iter().any(|s| s == "ZohoCRM.org.READ"),
        "the connection test reads the org"
    );
}

#[tokio::test]
async fn search_options_fields_and_org() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/crm/v8/Contacts/search"))
        .and(query_param("criteria", "(Email:equals:ann@x.io)"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"data": [{"id": "7", "Email": "ann@x.io"}]})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/crm/v8/Leads/search"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    Mock::given(method("GET")).and(path("/crm/v8/settings/modules"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"modules": [{"api_name": "Deals", "plural_label": "Deals", "api_supported": true}]})))
        .mount(&server).await;
    Mock::given(method("GET"))
        .and(path("/crm/v8/org"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"org": [{"company_name": "Acme", "id": "111", "domain_name": "org111"}]}),
        ))
        .mount(&server)
        .await;
    mount_fields(&server).await;
    let (cx, _) = zoho_ctx(&server);
    let c = ZohoCrmConnector::new();
    let found = c
        .execute(
            &cx,
            "search_records",
            json!({"module": "Contacts", "field": "Email", "value": "ann@x.io"}),
        )
        .await
        .unwrap();
    assert_eq!(
        (found["count"].as_u64(), found["first"]["id"].as_str()),
        (Some(1), Some("7"))
    );
    let none = c
        .execute(
            &cx,
            "search_records",
            json!({"module": "Leads", "criteria": "(Email:equals:x)"}),
        )
        .await
        .unwrap();
    assert_eq!(none["count"], 0);
    assert_eq!(
        c.options(&cx, "modules", &json!({})).await.unwrap()[0].value,
        "Deals"
    );
    let fields = c
        .fields(&cx, "module_fields", &json!({"module": "Deals"}))
        .await
        .unwrap();
    assert!(fields[0].required);
    assert_eq!(fields[1].kind, FieldKind::Select);
    assert_eq!(c.test_connection(&cx).await.unwrap()["org_name"], "Acme");
}
