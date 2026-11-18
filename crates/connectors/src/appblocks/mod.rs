//! `appblocks` connector: any Appblocks app (FSM, HMS, …) through its metadata-driven REST API.
//! See `docs/plan/13-mvp-zoho-crm-appblocks.md` §5.1.

mod client;
mod fields;
mod manifest;
mod records;
mod triggers;

use weft_connector_sdk::{EventBatch, InboundRequest, PollResult, RecordObjects, prelude::*};

use crate::util::{object, opt_bool, opt_f64, opt_str, req_str};

/// The Appblocks connector.
#[derive(Debug)]
pub struct AppblocksConnector {
    manifest: ConnectorManifest,
}

impl AppblocksConnector {
    #[must_use]
    pub fn new() -> Self {
        Self {
            manifest: manifest::manifest(),
        }
    }
}

impl Default for AppblocksConnector {
    fn default() -> Self {
        Self::new()
    }
}

fn values(input: &Value) -> Result<Map<String, Value>, ConnectorError> {
    let map = object(input, "values");
    if map.is_empty() {
        Err(ConnectorError::invalid_field(
            "values",
            "Map at least one field",
        ))
    } else {
        Ok(map)
    }
}

fn skip(input: &Value) -> bool {
    opt_bool(input, "skip_workflows").unwrap_or(true)
}

fn limit(input: &Value) -> u32 {
    // Clamped to the API's page size; the cast cannot truncate.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = opt_f64(input, "limit")
        .unwrap_or(10.0)
        .clamp(1.0, f64::from(records::PAGE_LIMIT)) as u32;
    n
}

#[async_trait]
impl Connector for AppblocksConnector {
    /// Appblocks API tokens: 300 requests/min and 10,000 per org per day (doc 13 §1).
    fn default_limits(&self) -> ConnectionLimits {
        ConnectionLimits {
            requests_per_minute: Some(300),
            max_concurrent: Some(10),
            daily_requests: Some(10_000),
        }
    }

    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    async fn execute(
        &self,
        ctx: &ActionCtx,
        action: &str,
        input: Value,
    ) -> Result<Value, ConnectorError> {
        match action {
            "create_record" => {
                let entity = records::create(
                    ctx,
                    &req_str(&input, "module")?,
                    &values(&input)?,
                    skip(&input),
                )
                .await?;
                Ok(json!({"id": records::entity_id(&entity)?, "record": entity}))
            }
            "update_record" => {
                let id = req_str(&input, "record_id")?;
                let entity = records::update(
                    ctx,
                    &req_str(&input, "module")?,
                    &id,
                    &values(&input)?,
                    skip(&input),
                )
                .await?;
                Ok(json!({"id": id, "record": entity}))
            }
            "upsert_record" => {
                let key = opt_str(&input, "duplicate_check_fields")
                    .unwrap_or_else(|| "external_id".into());
                let out = records::upsert(
                    ctx,
                    &req_str(&input, "module")?,
                    &values(&input)?,
                    &key,
                    skip(&input),
                )
                .await?;
                Ok(
                    json!({"id": records::entity_id(&out)?, "action": out.get("action"), "record": out.get("data")}),
                )
            }
            "get_record" => {
                let id = req_str(&input, "record_id")?;
                let entity = records::get(ctx, &req_str(&input, "module")?, &id)
                    .await?
                    .ok_or_else(|| ConnectorError::NotFound {
                        message: format!("record {id} not found"),
                    })?;
                Ok(json!({"id": id, "record": entity}))
            }
            "search_records" => {
                let value = input.get("value").cloned().unwrap_or(Value::Null);
                let rows = records::search_eq(
                    ctx,
                    &req_str(&input, "module")?,
                    &req_str(&input, "field")?,
                    &value,
                    limit(&input),
                )
                .await?;
                Ok(json!({"count": rows.len(), "first": rows.first(), "records": rows}))
            }
            "create_child_record" => {
                let parent_id = req_str(&input, "parent_id")?;
                records::create_children(
                    ctx,
                    &req_str(&input, "parent_module")?,
                    &parent_id,
                    &req_str(&input, "child_module")?,
                    &[Value::Object(values(&input)?)],
                    skip(&input),
                )
                .await?;
                Ok(json!({"created": true, "parent_id": parent_id}))
            }
            other => Err(ConnectorError::unsupported(other)),
        }
    }

    async fn test_connection(&self, ctx: &ActionCtx) -> Result<Value, ConnectorError> {
        let modules = records::modules(ctx).await?;
        Ok(json!({"modules": modules.as_array().map_or(0, Vec::len)}))
    }

    async fn options(
        &self,
        ctx: &ActionCtx,
        source: &str,
        input: &Value,
    ) -> Result<Vec<OptionItem>, ConnectorError> {
        match source {
            "modules" => Ok(fields::module_options(&records::modules(ctx).await?)),
            "module_field_names" => {
                let specs =
                    fields::field_specs(&records::fields(ctx, &req_str(input, "module")?).await?);
                Ok(specs
                    .into_iter()
                    .map(|f| OptionItem::new(f.key, f.label))
                    .collect())
            }
            other => Err(ConnectorError::unsupported(format!("options `{other}`"))),
        }
    }

    async fn fields(
        &self,
        ctx: &ActionCtx,
        source: &str,
        input: &Value,
    ) -> Result<Vec<FieldSpec>, ConnectorError> {
        let module = match source {
            "module_fields" => req_str(input, "module")?,
            "child_module_fields" => req_str(input, "child_module")?,
            other => return Err(ConnectorError::unsupported(format!("fields `{other}`"))),
        };
        Ok(fields::field_specs(&records::fields(ctx, &module).await?))
    }

    async fn poll(
        &self,
        ctx: &ActionCtx,
        trigger: &str,
        input: &Value,
        cursor: Option<&Value>,
    ) -> Result<PollResult, ConnectorError> {
        match trigger {
            "record_event" | "record_changed" => triggers::poll(ctx, input, cursor).await,
            other => Err(ConnectorError::unsupported(format!(
                "polling for `{other}`"
            ))),
        }
    }

    async fn handle_event(
        &self,
        ctx: &ActionCtx,
        trigger: &str,
        input: &Value,
        request: &InboundRequest,
        verify_token: &str,
    ) -> Result<EventBatch, ConnectorError> {
        match trigger {
            "record_event" => triggers::handle_event(ctx, input, request, verify_token).await,
            other => Err(ConnectorError::unsupported(format!("events for `{other}`"))),
        }
    }

    fn records(&self) -> Option<&dyn RecordObjects> {
        Some(self)
    }
}
