//! The Zoho CRM connector manifest.

use weft_connector_sdk::{ClientAuth, OAuth2Spec, prelude::*};

fn module_field() -> FieldSpec {
    FieldSpec::select("module", "Module")
        .required()
        .options_from("modules")
        .not_mappable()
}

fn values_field() -> FieldSpec {
    FieldSpec::new("values", "Fields", FieldKind::Object)
        .required()
        .fields_from("module_fields")
        .depends_on(&["module"])
}

fn trigger_workflows_field() -> FieldSpec {
    FieldSpec::new("trigger_workflows", "Run Zoho workflows", FieldKind::Boolean)
        .default_value(json!(false))
        .help("Off sends \"trigger\": [] so Zoho workflow rules, approvals and blueprints do not fire (prevents loops)")
        .advanced()
}

fn record_id_field() -> FieldSpec {
    FieldSpec::string("record_id", "Record ID").required()
}

fn record_sample() -> Value {
    json!({"id": "4876876000000624001", "record": {"id": "4876876000000624001", "Modified_Time": "2026-01-01T10:00:00+05:30", "Created_Time": "2026-01-01T10:00:00+05:30"}})
}

fn actions() -> Vec<ActionSpec> {
    vec![
        ActionSpec::new("create_record", "Create record", "Create a record in any Zoho CRM module.")
            .field(module_field())
            .field(values_field())
            .field(trigger_workflows_field())
            .output(record_sample()),
        ActionSpec::new("update_record", "Update record", "Update fields of an existing record.")
            .field(module_field())
            .field(record_id_field())
            .field(values_field())
            .field(trigger_workflows_field())
            .output(record_sample())
            .idempotent(),
        ActionSpec::new("upsert_record", "Create or update record", "Insert, or update the record matching the duplicate-check fields.")
            .field(module_field())
            .field(values_field())
            .field(FieldSpec::string("duplicate_check_fields", "Match on fields").help("Comma-separated api names, e.g. Email; empty uses Zoho's defaults"))
            .field(trigger_workflows_field())
            .output(json!({"id": "4876876000000624001", "action": "insert", "duplicate_field": null, "record": {"id": "4876876000000624001"}}))
            .idempotent(),
        ActionSpec::new("get_record", "Get record", "Fetch one record by ID.")
            .field(module_field())
            .field(record_id_field())
            .output(json!({"id": "4876876000000624001", "record": {"id": "4876876000000624001", "Deal_Name": "Acme renewal", "Stage": "Closed Won"}}))
            .idempotent(),
        ActionSpec::new("search_records", "Find records", "Search with Zoho criteria, or where a field equals a value.")
            .field(module_field())
            .field(FieldSpec::string("criteria", "Criteria").placeholder("(Email:equals:ann@example.com)").help("Zoho search criteria; leave empty to use Field + Value"))
            .field(FieldSpec::select("field", "Field").options_from("module_field_names").depends_on(&["module"]))
            .field(FieldSpec::string("value", "Value"))
            .field(FieldSpec::new("limit", "Max records", FieldKind::Number).default_value(json!(10)).advanced())
            .output(json!({"records": [{"id": "4876876000000624001"}], "count": 1, "first": {"id": "4876876000000624001"}}))
            .idempotent(),
        ActionSpec::new("add_note", "Add note", "Attach a note to a record.")
            .field(module_field())
            .field(record_id_field())
            .field(FieldSpec::string("title", "Title"))
            .field(FieldSpec::text("content", "Content").required())
            .output(json!({"id": "4876876000000700001", "record": {"id": "4876876000000700001"}})),
        ActionSpec::new("create_task", "Create task", "Create a task, optionally related to a record.")
            .field(FieldSpec::string("subject", "Subject").required())
            .field(FieldSpec::new("due_date", "Due date", FieldKind::Datetime).help("YYYY-MM-DD"))
            .field(FieldSpec::select("module", "Related module").options_from("modules").not_mappable())
            .field(FieldSpec::string("record_id", "Related record ID"))
            .field(FieldSpec::text("description", "Description"))
            .output(json!({"id": "4876876000000800001", "record": {"id": "4876876000000800001"}})),
    ]
}

fn triggers() -> Vec<TriggerSpec> {
    vec![
        TriggerSpec::new("record_event", "Record created / edited / deleted", "Instant via Zoho CRM notification channels, renewed automatically; reconciled by polling.", TriggerKind::Instant)
            .field(module_field())
            .field(
                FieldSpec::select("events", "Events")
                    .options(&[("create", "Created"), ("edit", "Edited"), ("delete", "Deleted")])
                    .multiple()
                    .default_value(json!(["create", "edit"]))
                    .not_mappable(),
            )
            .output(json!({"module": "Deals", "operation": "edit", "id": "4876876000000624001",
                "record": {"id": "4876876000000624001", "Deal_Name": "Acme renewal", "Stage": "Closed Won", "Amount": 12000, "Modified_Time": "2026-01-01T10:00:00+05:30"},
                "affected_fields": ["Stage"]}))
            .with_polling(),
    ]
}

/// The manifest; `accounts`: the operator's accounts server override (testing), else Zoho's (US) sign-in page.
pub(super) fn manifest(accounts: Option<&str>) -> ConnectorManifest {
    let oauth2 = OAuth2Spec {
        authorize_url: format!(
            "{}/oauth/v2/auth",
            accounts.unwrap_or("https://accounts.zoho.com")
        ),
        token_url: "{{ config.accounts_server }}/oauth/v2/token".into(),
        scopes: [
            "ZohoCRM.modules.ALL",
            "ZohoCRM.settings.READ",
            "ZohoCRM.notifications.ALL",
            "ZohoCRM.coql.READ",
            "ZohoCRM.users.READ",
            // The connection test reads the org (`GET /org`).
            "ZohoCRM.org.READ",
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect(),
        scope_separator: ",".into(),
        extra_authorize_params: vec![
            ("access_type".into(), "offline".into()),
            ("prompt".into(), "consent".into()),
        ],
        pkce: false,
        client_auth: ClientAuth::Body,
    };
    let edition = FieldSpec::select("edition", "Zoho CRM edition")
        .options(&[
            ("free", "Free"),
            ("standard", "Standard"),
            ("professional", "Professional"),
            ("enterprise", "Enterprise / Zoho One"),
            ("ultimate", "Ultimate / CRM Plus"),
        ])
        .default_value(json!("enterprise"))
        .help("Sets API credit and concurrency budgets")
        .not_mappable();
    ConnectorManifest {
        key: "zoho_crm".into(),
        name: "Zoho CRM".into(),
        description:
            "Leads, contacts, accounts, deals and any custom module in Zoho CRM (all data centres)."
                .into(),
        icon: "🟥".into(),
        category: "CRM".into(),
        auth: AuthSpec {
            kind: weft_connector_sdk::AuthKind::Oauth2,
            fields: vec![edition],
            oauth2: Some(oauth2),
        },
        actions: actions(),
        triggers: triggers(),
        docs_url: Some("https://www.zoho.com/crm/developer/docs/api/v8/".into()),
    }
}
