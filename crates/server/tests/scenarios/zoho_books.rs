//! Zoho Books (and Zoho Invoice, the same API) through "Any app (OAuth 2.0 sign-in)": Zoho's token prefix
//! (`Zoho-oauthtoken`) and the organisation on every call, and an FSM-style invoice flow that creates the Zoho
//! invoice once and updates it after that (found by its reference number, so there are no duplicates).

use serde_json::{Value, json};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_partial_json, body_string_contains, header, method, path, query_param},
};

use crate::{any_app::param, harness::TestApp};

/// A Zoho Books API call: Zoho's token prefix and the organisation are required, as at Zoho.
fn books(verb: &str, at: &str) -> wiremock::MockBuilder {
    Mock::given(method(verb))
        .and(path(format!("/books/v3{at}")))
        .and(header("authorization", "Zoho-oauthtoken at-1"))
        .and(query_param("organization_id", "777"))
}

/// Signs in to the mock Zoho accounts server; returns the connection ID.
async fn connect(app: &TestApp, zoho: &MockServer) -> String {
    Mock::given(method("POST"))
        .and(path("/oauth/v2/token"))
        .and(body_string_contains("grant_type=authorization_code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"access_token": "at-1", "refresh_token": "rt-1", "expires_in": 3600, "api_domain": zoho.uri()}),
        ))
        .mount(zoho)
        .await;
    books("GET", "/organizations")
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"organizations": [{"organization_id": "777"}]})),
        )
        .mount(zoho)
        .await;
    let started = app
        .ok(
            "POST",
            "/api/v1/connections/oauth/start",
            Some(json!({"connector": "http_oauth", "name": "Zoho Books", "return_to": "http://app.weft.test/connections/callback",
                "fields": {"base_url": format!("{}/books/v3", zoho.uri()),
                    "authorize_url": format!("{}/oauth/v2/auth", zoho.uri()),
                    "token_url": format!("{}/oauth/v2/token", zoho.uri()),
                    "client_id": "1000.CLIENT", "client_secret": "zoho-secret",
                    "scopes": "ZohoBooks.invoices.ALL ZohoBooks.contacts.ALL",
                    "authorize_params": {"access_type": "offline", "prompt": "consent"},
                    "token_prefix": "Zoho-oauthtoken", "default_query": {"organization_id": "777"},
                    "test_path": "/organizations"}})),
        )
        .await;
    let url = started["authorize_url"].as_str().unwrap();
    assert!(
        url.contains("access_type=offline"),
        "Zoho returns a refresh token only for offline access: {url}"
    );
    let state = param(url, "state");
    let back = app
        .redirect(&format!("/oauth/callback?state={state}&code=code-1"))
        .await;
    let finish = param(&back, "finish");
    let done = app
        .ok(
            "POST",
            "/api/v1/connections/oauth/finish",
            Some(json!({"state": state, "finish": finish})),
        )
        .await;
    done["connection_id"].as_str().unwrap().to_owned()
}

/// FSM invoice → Zoho Books invoice: find it by reference number; update it if it is there, else create it.
fn invoice_flow(cxn: &str) -> Value {
    let call = |key: &str, input: Value| json!({"key": key, "type": "action", "connector": "http_oauth", "action": "api_call", "connection_id": cxn, "input": input});
    let body = json!({"customer_id": "{{ trigger.customer_id }}", "reference_number": "{{ trigger.invoice_number }}",
        "line_items": "{{ trigger.lines }}"});
    json!({"trigger": {"type": "manual"}, "steps": [
        call("find", json!({"method": "GET", "path": "/invoices", "query": {"reference_number": "{{ trigger.invoice_number }}"}})),
        {"key": "exists", "type": "decision", "branches": [
            {"key": "found", "condition": {"left": "{{ steps.find.body.invoices | length }}", "op": "gt", "right": 0},
             "steps": [call("update", json!({"method": "PUT", "path": "/invoices/{{ steps.find.body.invoices[0].invoice_id }}", "body": body}))]}
        ], "otherwise": [call("create", json!({"method": "POST", "path": "/invoices", "body": body}))]}
    ]})
}

#[tokio::test]
async fn an_fsm_invoice_becomes_one_zoho_books_invoice_created_once_then_updated() {
    let app = TestApp::spawn().await;
    let zoho = MockServer::start().await;
    let cxn = connect(&app, &zoho).await;
    let status = app
        .ok("GET", &format!("/api/v1/connections/{cxn}"), None)
        .await;
    assert_eq!(
        status["status"], "active",
        "the connection test reached Zoho with its prefix and org: {status}"
    );
    books("GET", "/invoices")
        .and(query_param("reference_number", "INV-9"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"invoices": []})))
        .up_to_n_times(1)
        .mount(&zoho)
        .await;
    books("GET", "/invoices")
        .and(query_param("reference_number", "INV-9"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"invoices": [{"invoice_id": "5001", "reference_number": "INV-9"}]}),
        ))
        .mount(&zoho)
        .await;
    let lines = json!([{"name": "Repair visit", "rate": 120, "quantity": 1}]);
    books("POST", "/invoices")
        .and(body_partial_json(
            json!({"customer_id": "4600001", "reference_number": "INV-9", "line_items": lines}),
        ))
        .respond_with(
            ResponseTemplate::new(201)
                .set_body_json(json!({"code": 0, "invoice": {"invoice_id": "5001"}})),
        )
        .expect(1)
        .mount(&zoho)
        .await;
    books("PUT", "/invoices/5001")
        .and(body_partial_json(json!({"reference_number": "INV-9"})))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"code": 0, "invoice": {"invoice_id": "5001"}})),
        )
        .expect(1)
        .mount(&zoho)
        .await;
    let flow = app
        .live_flow("FSM invoice → Zoho Books", invoice_flow(&cxn))
        .await;
    let payload = json!({"invoice_number": "INV-9", "customer_id": "4600001", "lines": lines});
    let mut ran = Vec::new();
    for _ in 0..2 {
        let r = app
            .ok(
                "POST",
                &format!("/api/v1/flows/{}/run", flow["id"].as_str().unwrap()),
                Some(json!({"payload": payload})),
            )
            .await;
        app.drain().await;
        let run = app
            .ok(
                "GET",
                &format!("/api/v1/executions/{}", r["execution_id"].as_str().unwrap()),
                None,
            )
            .await;
        assert_eq!(run["status"], "succeeded", "{run}");
        ran.push(
            run["steps"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|s| s["step_key"].as_str().map(str::to_owned))
                .collect::<Vec<_>>(),
        );
    }
    assert!(
        ran[0].contains(&"create".to_owned()) && !ran[0].contains(&"update".to_owned()),
        "{ran:?}"
    );
    assert!(
        ran[1].contains(&"update".to_owned()) && !ran[1].contains(&"create".to_owned()),
        "{ran:?}"
    );
    zoho.verify().await;
    app.cleanup().await;
}
