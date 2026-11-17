//! The `core.sync` manifest (actions run in-platform via `sync::execute`).

use weft_connector_sdk::prelude::*;

/// Manifest-only connector so the catalog, validation and UI know the sync actions.
#[derive(Debug)]
pub struct SyncConnector {
    manifest: ConnectorManifest,
}

fn conn_field(key: &str, label: &str) -> FieldSpec {
    FieldSpec::select(key, label)
        .required()
        .options_from("connections")
        .not_mappable()
}

impl SyncConnector {
    #[must_use]
    pub fn new() -> Self {
        let upsert = ActionSpec::new("upsert_linked_record", "Sync record (create or update linked)", "Create the matching record in the target app, or update it if it is already linked. Skips unchanged data, never creates duplicates.")
            .field(FieldSpec::string("link_set", "Link name").required().not_mappable().help("Names this pairing, e.g. crm_accounts__fsm_accounts. Use the same name in lookups."))
            .field(conn_field("source_connection_id", "Source connection"))
            .field(FieldSpec::select("source_object", "Source module").required().options_from("source_objects").depends_on(&["source_connection_id"]).not_mappable())
            .field(FieldSpec::string("source_record_id", "Source record ID").required().placeholder("{{ trigger.id }}"))
            .field(conn_field("target_connection_id", "Target connection"))
            .field(FieldSpec::select("target_object", "Target module").required().options_from("target_objects").depends_on(&["target_connection_id"]).not_mappable())
            .field(FieldSpec::new("values", "Target fields", FieldKind::Object).required().fields_from("target_fields").depends_on(&["target_connection_id", "target_object"]))
            .field(FieldSpec::string("match_field", "Match existing by field").help("Before creating, look for a target record with this field (e.g. external_id or Email)").advanced())
            .field(FieldSpec::string("match_value", "Match value").advanced())
            .field(FieldSpec::new("lookups", "Lookups through links", FieldKind::Json).advanced().help("[{\"field\": \"account_id\", \"link_set\": \"crm_accounts__fsm_accounts\", \"source_record_id\": \"{{ trigger.record.Account_Name.id }}\"}]"))
            .output(json!({"action": "created", "target_id": "01J…", "link_set": "crm_accounts__fsm_accounts"}));
        let resolve = ActionSpec::new(
            "resolve_link",
            "Find linked record (source → target)",
            "Look up the target record linked to a source record.",
        )
        .field(
            FieldSpec::string("link_set", "Link name")
                .required()
                .not_mappable(),
        )
        .field(conn_field("source_connection_id", "Source connection"))
        .field(
            FieldSpec::select("source_object", "Source module")
                .required()
                .options_from("source_objects")
                .depends_on(&["source_connection_id"])
                .not_mappable(),
        )
        .field(FieldSpec::string("source_record_id", "Source record ID").required())
        .output(json!({"found": true, "target_id": "01J…", "target_object": "requests"}))
        .idempotent();
        let reverse = ActionSpec::new(
            "find_linked_record",
            "Find linked record (target → source)",
            "Look up the source record linked to a target record (e.g. FSM job → Zoho Deal).",
        )
        .field(
            FieldSpec::string("link_set", "Link name")
                .required()
                .not_mappable(),
        )
        .field(conn_field("target_connection_id", "Target connection"))
        .field(
            FieldSpec::select("target_object", "Target module")
                .required()
                .options_from("target_objects")
                .depends_on(&["target_connection_id"])
                .not_mappable(),
        )
        .field(FieldSpec::string("target_record_id", "Target record ID").required())
        .output(
            json!({"found": true, "source_id": "4876876000000624001", "source_object": "Deals"}),
        )
        .idempotent();
        let save = ActionSpec::new(
            "save_link",
            "Save link",
            "Record that two records correspond to each other.",
        )
        .field(
            FieldSpec::string("link_set", "Link name")
                .required()
                .not_mappable(),
        )
        .field(conn_field("source_connection_id", "Source connection"))
        .field(FieldSpec::string("source_object", "Source module").required())
        .field(FieldSpec::string("source_record_id", "Source record ID").required())
        .field(conn_field("target_connection_id", "Target connection"))
        .field(FieldSpec::string("target_object", "Target module").required())
        .field(FieldSpec::string("target_record_id", "Target record ID").required())
        .output(json!({"saved": true}))
        .idempotent();
        Self {
            manifest: ConnectorManifest {
                key: super::CONNECTOR_KEY.into(),
                name: "Sync".into(),
                description:
                    "Keep records in two apps linked: create-or-update without duplicates or loops."
                        .into(),
                icon: "🔗".into(),
                category: "Utilities".into(),
                auth: AuthSpec::none(),
                actions: vec![upsert, resolve, reverse, save],
                triggers: Vec::new(),
                docs_url: None,
            },
        }
    }
}

impl Default for SyncConnector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Connector for SyncConnector {
    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    async fn execute(
        &self,
        _ctx: &ActionCtx,
        action: &str,
        _input: Value,
    ) -> Result<Value, ConnectorError> {
        Err(ConnectorError::unsupported(format!(
            "{action} runs inside the platform"
        )))
    }
}
