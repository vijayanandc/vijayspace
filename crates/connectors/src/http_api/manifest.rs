//! The "Any app (HTTP API)" manifest: a connection to any REST API (base URL plus one way of signing requests), a
//! "Make an API call" action and a "New items" polling trigger. Like Zoho Flow's custom apps: credentials live in
//! the connection (encrypted), never in a flow.

use weft_connector_sdk::{AuthKind, prelude::*};

/// Every way a request can carry the connection's credentials.
pub(super) const AUTH_TYPES: [(&str, &str); 6] = [
    ("none", "No authentication"),
    ("api_key_header", "API key in a header"),
    ("api_key_query", "API key in the query string"),
    ("bearer", "Bearer token"),
    ("basic", "Username and password (Basic)"),
    ("oauth2_client_credentials", "OAuth 2.0 client credentials"),
];

/// Where a token goes: header name and prefix. `types`: the sign-in types that send a token (`None`: always).
fn token_fields(types: Option<&[&str]>) -> [FieldSpec; 2] {
    let only = |f: FieldSpec| match types {
        Some(t) => f.show_when("auth", t),
        None => f,
    };
    [
        only(FieldSpec::string("token_prefix", "Token prefix"))
            .default_value(json!("Bearer"))
            .help("The word before the token: Bearer for most apps, Zoho-oauthtoken for Zoho apps (Invoice, Books, CRM…), none for the token alone")
            .not_mappable(),
        only(FieldSpec::string("token_header", "Token header"))
            .default_value(json!("Authorization"))
            .help("The header that carries the token")
            .advanced()
            .not_mappable(),
    ]
}

/// Headers and query parameters on every call of a connection (plain, or secret and stored encrypted).
fn every_call_fields() -> [FieldSpec; 3] {
    [
        FieldSpec::new("default_headers", "Headers on every call", FieldKind::KeyValue)
            .help("e.g. Accept, an API version, or Zoho Invoice's X-com-zoho-invoice-organizationid. Not for credentials: use Secret headers")
            .advanced()
            .not_mappable(),
        FieldSpec::new("secret_headers", "Secret headers on every call", FieldKind::KeyValue)
            .secret()
            .help("Credential headers (e.g. X-Api-Secret): stored encrypted, never shown again, never logged")
            .advanced()
            .not_mappable(),
        FieldSpec::new("default_query", "Query parameters on every call", FieldKind::KeyValue)
            .help("e.g. Zoho Books' organization_id")
            .advanced()
            .not_mappable(),
    ]
}

fn auth_fields() -> Vec<FieldSpec> {
    let key_types = ["api_key_header", "api_key_query"];
    let token_types = ["bearer", "oauth2_client_credentials"];
    let mut fields = vec![
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
    ];
    fields.extend(token_fields(Some(&token_types)));
    fields.extend(every_call_fields());
    fields
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

fn changed_items_trigger() -> TriggerSpec {
    TriggerSpec::new(
        "changed_items",
        "New or changed items (polling)",
        "Checks a list endpoint on an interval and starts a run for each new item and each item that changed (e.g. an invoice marked paid).",
        TriggerKind::Polling,
    )
    .field(FieldSpec::string("path", "List path").required().placeholder("/invoices").not_mappable())
    .field(
        FieldSpec::new("query", "Query parameters", FieldKind::KeyValue)
            .help("Sort or filter so recent changes come first: only the items this answer holds are compared")
            .not_mappable(),
    )
    .field(FieldSpec::string("items_path", "Where the items are").placeholder("invoices").not_mappable())
    .field(FieldSpec::string("id_field", "ID field").default_value(json!("id")).not_mappable())
    .field(
        FieldSpec::string("version_field", "Changes with the item")
            .placeholder("last_modified_time")
            .help("A field that changes whenever the item does; empty = any change to the item's content")
            .not_mappable(),
    )
    .output(json!({"id": "1001", "change": "changed", "item": {"id": "1001", "status": "paid"}}))
}

fn oauth_fields() -> Vec<FieldSpec> {
    let mut fields = vec![
        FieldSpec::string("base_url", "Base URL")
            .required()
            .placeholder("https://api.example.com/v1")
            .help("Every call of this connection goes to a path under this address")
            .not_mappable(),
        FieldSpec::string("authorize_url", "Authorization URL")
            .required()
            .placeholder("https://app.example.com/oauth/authorize")
            .help("From the app's developer settings, as are the token URL and the client")
            .not_mappable(),
        FieldSpec::string("token_url", "Token URL")
            .required()
            .placeholder("https://app.example.com/oauth/token")
            .not_mappable(),
        FieldSpec::string("client_id", "Client ID").required().not_mappable(),
        FieldSpec::new("client_secret", "Client secret", FieldKind::Password)
            .required()
            .secret()
            .not_mappable(),
        FieldSpec::string("scopes", "Scopes")
            .placeholder("read write offline_access")
            .help("Space-separated. Ask for offline access (if the app has it) so Weft can refresh the sign-in")
            .not_mappable(),
        FieldSpec::select("client_auth", "Send the client secret")
            .options(&[("body", "In the request body"), ("basic", "As Basic authentication")])
            .default_value(json!("body"))
            .advanced()
            .not_mappable(),
        FieldSpec::new("pkce", "Use PKCE", FieldKind::Boolean)
            .default_value(json!(true))
            .advanced()
            .not_mappable(),
        FieldSpec::new("authorize_params", "Extra sign-in parameters", FieldKind::KeyValue)
            .help("e.g. access_type = offline, prompt = consent")
            .advanced()
            .not_mappable(),
        FieldSpec::string("test_path", "Path to test the connection")
            .placeholder("/me")
            .advanced()
            .not_mappable(),
    ];
    // The sign-in is always a token, so its prefix and header always apply.
    fields.extend(token_fields(None));
    fields.extend(every_call_fields());
    fields
}

/// "Any app (OAuth 2.0 sign-in)": the same action and trigger, signed with the token of a sign-in at the app,
/// through the app's own OAuth client (its settings are the connection's).
pub(super) fn oauth_manifest() -> ConnectorManifest {
    ConnectorManifest {
        key: "http_oauth".into(),
        name: "Any app (OAuth 2.0 sign-in)".into(),
        description: "Connect any app that signs in with OAuth 2.0, using an OAuth client you register at the app (redirect URI: this server's /oauth/callback).".into(),
        icon: "🔐".into(),
        category: "Custom apps".into(),
        auth: AuthSpec {
            kind: AuthKind::Oauth2,
            fields: oauth_fields(),
            oauth2: None,
        },
        actions: vec![call_action()],
        triggers: vec![new_items_trigger(), changed_items_trigger()],
        docs_url: None,
    }
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
        triggers: vec![new_items_trigger(), changed_items_trigger()],
        docs_url: None,
    }
}
