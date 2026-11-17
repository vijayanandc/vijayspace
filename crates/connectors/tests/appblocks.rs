//! `appblocks` connector against a mock Appblocks server.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{collections::BTreeMap, sync::Arc};

use serde_json::{Value, json};
use weft_connector_sdk::{
    ActionCtx, Connector, ConnectorError, FieldKind, InboundRequest,
    testing::{StaticConnection, ctx},
};
use weft_connectors::appblocks::AppblocksConnector;
use weft_http::{ReqwestSender, SsrfPolicy};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_json, header, method, path, query_param, query_param_is_missing},
};

fn appblocks_ctx(server: &MockServer) -> ActionCtx {
    let conn = StaticConnection::new(
        json!({"base_url": format!("{}/", server.uri()), "org_id": "12"}),
        json!({"api_token": "apb_x"}),
    );
    ctx(
        Arc::new(ReqwestSender::new(SsrfPolicy::allow_private()).unwrap()),
        Some(conn),
    )
}

fn authed(builder: wiremock::MockBuilder) -> wiremock::MockBuilder {
    builder
        .and(header("authorization", "Bearer apb_x"))
        .and(header("x-org-id", "12"))
}

#[tokio::test]
async fn create_record_skips_workflows_and_returns_the_entity() {
    let server = MockServer::start().await;
    authed(Mock::given(method("POST")).and(path("/api/requests")))
        .and(query_param("trigger", ""))
        .and(body_json(
            json!({"title": "Boiler service", "account_id": "01ACC"}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"api_id": "01REQ", "title": "Boiler service", "request_number": "REQ-0012"}),
        ))
        .expect(1)
        .mount(&server)
        .await;
    let input =
        json!({"module": "requests", "values": {"title": "Boiler service", "account_id": "01ACC"}});
    let out = AppblocksConnector::new()
        .execute(&appblocks_ctx(&server), "create_record", input)
        .await
        .unwrap();
    assert_eq!(out["id"], "01REQ");
    assert_eq!(out["record"]["request_number"], "REQ-0012");
}

#[tokio::test]
async fn update_without_skip_and_upsert_by_external_id() {
    let server = MockServer::start().await;
    authed(Mock::given(method("PATCH")).and(path("/api/jobs/01JOB")))
        .and(query_param_is_missing("trigger"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"api_id": "01JOB", "status": "Completed"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    authed(Mock::given(method("POST")).and(path("/api/accounts/upsert")))
        .and(query_param("duplicate_check_fields", "external_id"))
        .and(query_param("trigger", ""))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"action": "insert", "api_id": "01ACC", "data": {"api_id": "01ACC", "name": "Acme"}})))
        .expect(1)
        .mount(&server)
        .await;
    let c = AppblocksConnector::new();
    let cx = appblocks_ctx(&server);
    let upd = c.execute(&cx, "update_record", json!({"module": "jobs", "record_id": "01JOB", "values": {"status": "Completed"}, "skip_workflows": false})).await.unwrap();
    assert_eq!(upd["record"]["status"], "Completed");
    let up = c
        .execute(
            &cx,
            "upsert_record",
            json!({"module": "accounts", "values": {"name": "Acme", "external_id": "4876"}}),
        )
        .await
        .unwrap();
    assert_eq!(
        (up["id"].as_str(), up["action"].as_str()),
        (Some("01ACC"), Some("insert"))
    );
}

#[tokio::test]
async fn search_sends_eq_criteria() {
    let server = MockServer::start().await;
    authed(Mock::given(method("POST")).and(path("/api/contacts/search")))
        .and(query_param("limit", "5"))
        .and(body_json(json!({"criteria_pattern": "1", "criteria_details": [{"field_api_name": "email", "comparator": "eq", "value": "ann@x.io"}]})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{"api_id": "01CON", "email": "ann@x.io"}])))
        .mount(&server)
        .await;
    let out = AppblocksConnector::new()
        .execute(
            &appblocks_ctx(&server),
            "search_records",
            json!({"module": "contacts", "field": "email", "value": "ann@x.io", "limit": 5}),
        )
        .await
        .unwrap();
    assert_eq!(out["count"], 1);
    assert_eq!(out["first"]["api_id"], "01CON");
}

#[tokio::test]
async fn errors_are_mapped() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/accounts"))
        .respond_with(
            ResponseTemplate::new(409).set_body_json(
                json!({"error": "external_id already used", "code": "DUPLICATE_VALUE"}),
            ),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/jobs"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("X-RateLimit-Remaining", "0")
                .set_body_json(json!({"error": "organization daily API limit exceeded"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/invoices"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({"error": "invalid token"})))
        .mount(&server)
        .await;
    let c = AppblocksConnector::new();
    let cx = appblocks_ctx(&server);
    let run = |module: &'static str| {
        c.execute(
            &cx,
            "create_record",
            json!({"module": module, "values": {"name": "x"}}),
        )
    };
    assert!(
        matches!(run("accounts").await.unwrap_err(), ConnectorError::InvalidInput { message, .. } if message.contains("DUPLICATE_VALUE"))
    );
    // The daily cap resets at UTC midnight; test time is 2026-01-01T00:00:00Z.
    assert_eq!(
        run("jobs").await.unwrap_err().retry_after_secs(),
        Some(86_400)
    );
    assert!(matches!(
        run("invoices").await.unwrap_err(),
        ConnectorError::AuthExpired { .. }
    ));
    assert!(
        c.execute(
            &cx,
            "create_record",
            json!({"module": "jobs/../x", "values": {"a": 1}})
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn options_and_dynamic_fields() {
    let server = MockServer::start().await;
    authed(Mock::given(method("GET")).and(path("/api/modules")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{"name": "Jobs", "module_api_name": "jobs"}, {"name": "Accounts", "module_api_name": "accounts"}])))
        .mount(&server).await;
    authed(Mock::given(method("GET")).and(path("/api/fields")).and(query_param("module_api_name", "jobs")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"api_name": "status", "label_name": "Status", "data_type": "drop_down", "options": [{"value": "Planned", "label": "Planned"}]},
            {"api_name": "job_number", "label_name": "Job #", "data_type": "auto_number"}
        ])))
        .mount(&server).await;
    let c = AppblocksConnector::new();
    let cx = appblocks_ctx(&server);
    let modules = c.options(&cx, "modules", &json!({})).await.unwrap();
    assert_eq!(modules[0].value, "jobs");
    let fields = c
        .fields(&cx, "module_fields", &json!({"module": "jobs"}))
        .await
        .unwrap();
    assert_eq!(fields[0].kind, FieldKind::Select);
    assert!(fields[1].read_only);
    assert_eq!(c.test_connection(&cx).await.unwrap(), json!({"modules": 2}));
}

fn inbound(headers: &[(&str, &str)], body: Value) -> InboundRequest {
    InboundRequest {
        method: "POST".into(),
        headers: headers
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect(),
        query: BTreeMap::new(),
        body,
    }
}

#[tokio::test]
async fn workflow_webhook_events_are_verified_and_fetched() {
    let server = MockServer::start().await;
    authed(Mock::given(method("GET")).and(path("/api/jobs/01JOB")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"api_id": "01JOB", "status": "Completed", "modified_time": "2026-01-01T10:00:00+05:30"})))
        .expect(1)
        .mount(&server).await;
    let c = AppblocksConnector::new();
    let cx = appblocks_ctx(&server);
    let input = json!({"module": "jobs", "operation": "any"});
    let body = json!({"module": "jobs", "operation": "update", "api_id": "01JOB"});
    let bad = c
        .handle_event(
            &cx,
            "record_event",
            &input,
            &inbound(&[("x-weft-key", "nope")], body.clone()),
            "secret",
        )
        .await;
    assert!(bad.is_err());
    let batch = c
        .handle_event(
            &cx,
            "record_event",
            &input,
            &inbound(
                &[("x-weft-key", "secret"), ("idempotency-key", "evt-9")],
                body,
            ),
            "secret",
        )
        .await
        .unwrap();
    assert_eq!(batch.items.len(), 1);
    // The record version, not the delivery ID: polling builds the same key, so the
    // reconciliation poll does not re-run a version already delivered live.
    assert_eq!(batch.items[0].dedupe_key, "01JOB:2026-01-01T10:00:00+05:30");
    assert_eq!(batch.items[0].data["record"]["status"], "Completed");
    // Operation filter: a delete event is ignored when only updates are wanted; deletes are never fetched.
    let only_updates = json!({"module": "jobs", "operation": "update"});
    let del = json!({"module": "jobs", "operation": "delete", "api_id": "01JOB"});
    assert!(
        c.handle_event(
            &cx,
            "record_event",
            &only_updates,
            &inbound(&[("x-weft-key", "secret")], del.clone()),
            "secret"
        )
        .await
        .unwrap()
        .items
        .is_empty()
    );
    let deleted = c
        .handle_event(
            &cx,
            "record_event",
            &input,
            &inbound(&[("x-weft-key", "secret")], del),
            "secret",
        )
        .await
        .unwrap();
    assert_eq!(deleted.items[0].dedupe_key, "01JOB:deleted");
}

#[tokio::test]
async fn polling_starts_with_a_baseline_then_returns_changes() {
    let server = MockServer::start().await;
    authed(Mock::given(method("GET")).and(path("/api/accounts")))
        .and(query_param("sort_by", "modified_time"))
        .and(query_param("sort_type", "asc"))
        .and(query_param(
            "filter[modified_time][gte]",
            "2026-01-01T00:00:00+00:00",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"api_id": "01A", "modified_time": "2026-01-01T01:00:00+00:00"},
            {"api_id": "01B", "modified_time": "2026-01-01T02:00:00+00:00"}
        ])))
        .mount(&server)
        .await;
    let c = AppblocksConnector::new();
    let cx = appblocks_ctx(&server);
    let input = json!({"module": "accounts"});
    let baseline = c.poll(&cx, "record_changed", &input, None).await.unwrap();
    assert!(baseline.items.is_empty());
    assert_eq!(baseline.cursor, Some(json!("2026-01-01T00:00:00+00:00")));
    let next = c
        .poll(&cx, "record_changed", &input, baseline.cursor.as_ref())
        .await
        .unwrap();
    assert_eq!(
        next.items
            .iter()
            .map(|i| i.dedupe_key.as_str())
            .collect::<Vec<_>>(),
        vec![
            "01A:2026-01-01T01:00:00+00:00",
            "01B:2026-01-01T02:00:00+00:00"
        ]
    );
    assert_eq!(next.cursor, Some(json!("2026-01-01T02:00:00+00:00")));
}

#[tokio::test]
async fn record_objects_capability() {
    let server = MockServer::start().await;
    authed(Mock::given(method("POST")).and(path("/api/contacts/search")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{"api_id": "01CON", "email": "ann@x.io", "modified_time": "2026-01-01T00:00:00Z"}])))
        .mount(&server).await;
    authed(Mock::given(method("POST")).and(path("/api/contacts")))
        .and(query_param("trigger", ""))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"api_id": "01NEW"})))
        .mount(&server)
        .await;
    authed(Mock::given(method("GET")).and(path("/api/contacts/01GONE")))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"error": "not found"})))
        .mount(&server)
        .await;
    let c = AppblocksConnector::new();
    let cx = appblocks_ctx(&server);
    let records = c.records().unwrap();
    let found = records
        .find_by(&cx, "contacts", "email", &json!("ann@x.io"))
        .await
        .unwrap();
    assert_eq!(found[0].id, "01CON");
    let mut values = serde_json::Map::new();
    values.insert("email".into(), json!("bo@x.io"));
    assert_eq!(
        records.create(&cx, "contacts", &values).await.unwrap().id,
        "01NEW"
    );
    assert!(
        records
            .get(&cx, "contacts", "01GONE")
            .await
            .unwrap()
            .is_none()
    );
}
