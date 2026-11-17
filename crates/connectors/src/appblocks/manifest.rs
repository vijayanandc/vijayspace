//! The Appblocks connector manifest.

use weft_connector_sdk::prelude::*;

use super::triggers::SETUP;

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

fn skip_workflows_field() -> FieldSpec {
    FieldSpec::new(
        "skip_workflows",
        "Skip Appblocks workflows",
        FieldKind::Boolean,
    )
    .default_value(json!(true))
    .help("Recommended: prevents this write from firing workflows (and looping back into Weft)")
    .advanced()
}

fn record_sample() -> Value {
    json!({"id": "01HZX8P6M0Q4T2V9B3C7D5E1F0", "record": {"api_id": "01HZX8P6M0Q4T2V9B3C7D5E1F0", "name": "Acme Corp", "modified_time": "2026-01-01T10:00:00+05:30"}})
}

fn actions() -> Vec<ActionSpec> {
    vec![
        ActionSpec::new("create_record", "Create record", "Create a record in any Appblocks module.")
            .field(module_field())
            .field(values_field())
            .field(skip_workflows_field())
            .output(record_sample()),
        ActionSpec::new("update_record", "Update record", "Update fields of an existing record.")
            .field(module_field())
            .field(FieldSpec::string("record_id", "Record ID (api_id)").required())
            .field(values_field())
            .field(skip_workflows_field())
            .output(record_sample())
            .idempotent(),
        ActionSpec::new("upsert_record", "Create or update record", "Insert, or update the record matching a key field (e.g. external_id).")
            .field(module_field())
            .field(values_field())
            .field(FieldSpec::string("duplicate_check_fields", "Match on field").default_value(json!("external_id")).help("api_name of a unique field, e.g. external_id"))
            .field(skip_workflows_field())
            .output(json!({"id": "01HZX8P6M0Q4T2V9B3C7D5E1F0", "action": "insert", "record": {"api_id": "01HZX8P6M0Q4T2V9B3C7D5E1F0"}}))
            .idempotent(),
        ActionSpec::new("get_record", "Get record", "Fetch one record by its api_id.")
            .field(module_field())
            .field(FieldSpec::string("record_id", "Record ID (api_id)").required())
            .output(record_sample())
            .idempotent(),
        ActionSpec::new("search_records", "Find records", "Find records where a field equals a value.")
            .field(module_field())
            .field(FieldSpec::select("field", "Field").required().options_from("module_field_names").depends_on(&["module"]))
            .field(FieldSpec::string("value", "Value").required())
            .field(FieldSpec::new("limit", "Max records", FieldKind::Number).default_value(json!(10)).advanced())
            .output(json!({"records": [{"api_id": "01HZX8P6M0Q4T2V9B3C7D5E1F0"}], "count": 1, "first": {"api_id": "01HZX8P6M0Q4T2V9B3C7D5E1F0"}}))
            .idempotent(),
        ActionSpec::new("create_child_record", "Add child record", "Add a row to a child module (e.g. job materials, line items).")
            .field(FieldSpec::select("parent_module", "Parent module").required().options_from("modules").not_mappable())
            .field(FieldSpec::string("parent_id", "Parent record ID").required())
            .field(FieldSpec::select("child_module", "Child module").required().options_from("modules").not_mappable())
            .field(FieldSpec::new("values", "Fields", FieldKind::Object).required().fields_from("child_module_fields").depends_on(&["child_module"]))
            .field(skip_workflows_field())
            .output(json!({"created": true, "parent_id": "01HZX8P6M0Q4T2V9B3C7D5E1F0"})),
    ]
}

fn triggers() -> Vec<TriggerSpec> {
    vec![
        TriggerSpec::new("record_event", "Record created / updated / deleted", "Instant: an Appblocks workflow webhook notifies Weft.", TriggerKind::Instant)
            .field(module_field())
            .field(FieldSpec::select("operation", "Operation").options(&[("any", "Any"), ("create", "Created"), ("update", "Updated"), ("delete", "Deleted")]).default_value(json!("any")).not_mappable())
            .output(json!({"module": "jobs", "operation": "update", "id": "01HZX8P6M0Q4T2V9B3C7D5E1F0", "record": {"api_id": "01HZX8P6M0Q4T2V9B3C7D5E1F0", "status": "Completed"}}))
            .setup(SETUP)
            .with_polling(),
        TriggerSpec::new("record_changed", "New or updated record (polling)", "Checks the module for changes on an interval; no Appblocks setup needed.", TriggerKind::Polling)
            .field(module_field())
            .output(json!({"module": "accounts", "operation": "update", "id": "01HZX8P6M0Q4T2V9B3C7D5E1F0", "record": {"api_id": "01HZX8P6M0Q4T2V9B3C7D5E1F0"}})),
    ]
}

pub(super) fn manifest() -> ConnectorManifest {
    ConnectorManifest {
        key: "appblocks".into(),
        name: "Appblocks (FSM, HMS, …)".into(),
        description: "Any Appblocks app — Field Service, Hospital Management and more — through its metadata-driven API.".into(),
        icon: "🧩".into(),
        category: "Business apps".into(),
        auth: AuthSpec::fields(vec![
            FieldSpec::string("base_url", "App URL").required().placeholder("https://fsm.example.com").not_mappable(),
            FieldSpec::string("org_id", "Org ID").required().help("Appblocks numeric org ID").not_mappable(),
            FieldSpec::new("api_token", "API token", FieldKind::Password).required().secret().not_mappable().help("apb_… token from Settings → API tokens (needs module scopes)"),
        ]),
        actions: actions(),
        triggers: triggers(),
        docs_url: None,
    }
}
