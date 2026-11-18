//! The "Any app (HTTP API)" manifest: a connection to any REST API (base URL plus one way of signing requests), a
//! "Make an API call" action and a "New items" polling trigger. Like Zoho Flow's custom apps: credentials live in
//! the connection (encrypted), never in a flow.

use weft_connector_sdk::prelude::*;

/// Every way a request can carry the connection's credentials.
pub(super) const AUTH_TYPES: [(&str, &str); 6] = [
    ("none", "No authentication"),
    ("api_key_header", "API key in a header"),
    ("api_key_query", "API key in the query string"),
    ("bearer", "Bearer token"),
    ("basic", "Username and password (Basic)"),
    ("oauth2_client_credentials", "OAuth 2.0 client credentials"),
];

fn auth_fields() -> Vec<FieldSpec> {
    let key_types = ["api_key_header", "api_key_query"];
    vec![
        FieldSpec::string("base_url", "Base URL")
            .required()
            .placeholder("https://api.example.com/v1")
            .help("Every call of this connection goes to a path under this address")
            .not_mappable(),
        FieldSpec::select("auth", "Authentication")
            .required()
            .options(&AUTH_TYPES)
            .default_value(json!("api_key_header"))
            .not_mappable(),
        FieldSpec::string("header_name", "Header name")
            .default_value(json!("X-API-Key"))
            .show_when("auth", &["api_key_header"])
            .not_mappable(),
        FieldSpec::string("query_name", "Query parameter name")
            .default_value(json!("api_key"))
            .show_when("auth", &["api_key_query"])
            .not_mappable(),
        FieldSpec::new("api_key", "API key", FieldKind::Password)
            .required()
            .secret()
            .show_when("auth", &key_types)
            .not_mappable(),
        FieldSpec::new("token", "Token", FieldKind::Password)
            .required()
            .secret()
            .show_when("auth", &["bearer"])
            .not_mappable(),
        FieldSpec::string("username", "Username")
            .required()
            .show_when("auth", &["basic"])
            .not_mappable(),
        FieldSpec::new("password", "Password", FieldKind::Password)
            .required()
            .secret()
            .show_when("auth", &["basic"])
            .not_mappable(),
        FieldSpec::string("token_url", "Token URL")
            .required()
            .placeholder("https://auth.example.com/oauth/token")
            .show_when("auth", &["oauth2_client_credentials"])
            .not_mappable(),
        FieldSpec::string("client_id", "Client ID")
            .required()
            .show_when("auth", &["oauth2_client_credentials"])
            .not_mappable(),
        FieldSpec::new("client_secret", "Client secret", FieldKind::Password)
            .required()
            .secret()
            .show_when("auth", &["oauth2_client_credentials"])
            .not_mappable(),
        FieldSpec::string("scope", "Scopes")
            .help("Space-separated, if the app asks for them")
            .show_when("auth", &["oauth2_client_credentials"])
            .not_mappable(),
        FieldSpec::string("test_path", "Path to test the connection")
            .placeholder("/me")
            .help("A GET that needs valid credentials; empty = the base URL")
            .advanced()
            .not_mappable(),
        FieldSpec::new(
            "default_headers",
            "Headers on every call",
            FieldKind::KeyValue,
        )
        .help("e.g. Accept or an API version header (no credentials here)")
        .advanced()
        .not_mappable(),
    ]
}

fn call_action() -> ActionSpec {
    ActionSpec::new(
        "api_call",
        "Make an API call",
        "Call any endpoint of the app; the connection signs the request.",
    )
    .field(
        FieldSpec::select("method", "Method")
            .options(&[("GET", "GET"), ("POST", "POST"), ("PUT", "PUT"), ("PATCH", "PATCH"), ("DELETE", "DELETE")])
            .default_value(json!("GET")),
    )
    .field(
        FieldSpec::string("path", "Path")
            .required()
            .placeholder("/customers/{{ trigger.body.id }}")
            .help("Relative to the connection's base URL"),
    )
    .field(FieldSpec::new("query", "Query parameters", FieldKind::KeyValue))
    .field(FieldSpec::new("headers", "Headers", FieldKind::KeyValue).advanced())
    .field(
        FieldSpec::select("body_type", "Body type")
            .options(&[("json", "JSON"), ("form", "Form"), ("text", "Text"), ("none", "None")])
            .default_value(json!("json"))
            .show_when("method", &["POST", "PUT", "PATCH", "DELETE"]),
    )
    .field(
        FieldSpec::new("body", "Body", FieldKind::Json)
            .show_when("method", &["POST", "PUT", "PATCH", "DELETE"]),
    )
    .field(
        FieldSpec::new("fail_on_error_status", "Fail on non-2xx status", FieldKind::Boolean)
            .default_value(json!(true))
            .advanced(),
    )
    .field(FieldSpec::new("send_idempotency_key", "Send Idempotency-Key header", FieldKind::Boolean).advanced())
    .field(
        FieldSpec::new("timeout_secs", "Timeout (seconds)", FieldKind::Number)
            .default_value(json!(30))
            .advanced(),
    )
    .output(json!({"status": 200, "headers": {"content-type": "application/json"}, "body": {"id": "123"}}))
}

fn new_items_trigger() -> TriggerSpec {
    TriggerSpec::new(
        "new_items",
        "New items (polling)",
        "Checks a list endpoint on an interval and starts a run for each item not seen before.",
        TriggerKind::Polling,
    )
    .field(
        FieldSpec::string("path", "List path")
            .required()
            .placeholder("/orders")
            .not_mappable(),
    )
    .field(FieldSpec::new("query", "Query parameters", FieldKind::KeyValue).not_mappable())
    .field(
        FieldSpec::string("items_path", "Where the items are")
            .placeholder("data.items")
            .help("Dotted path to the list in the answer; empty when the answer is the list itself")
            .not_mappable(),
    )
    .field(
        FieldSpec::string("id_field", "ID field")
            .default_value(json!("id"))
            .help("The field that identifies an item")
            .not_mappable(),
    )
    .output(json!({"id": "1001", "item": {"id": "1001", "status": "new"}}))
}

pub(super) fn manifest() -> ConnectorManifest {
    ConnectorManifest {
        key: "http_api".into(),
        name: "Any app (HTTP API)".into(),
        description: "Connect any app with a REST API: API key, bearer token, Basic or OAuth 2.0 client credentials. Use Webhook triggers for apps that push events.".into(),
        icon: "🔌".into(),
        category: "Custom apps".into(),
        auth: AuthSpec::fields(auth_fields()),
        actions: vec![call_action()],
        triggers: vec![new_items_trigger()],
        docs_url: None,
    }
}
