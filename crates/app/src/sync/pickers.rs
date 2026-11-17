//! Dynamic options/fields for `core.sync` (connections with record support, their modules and fields).

use serde_json::Value;
use weft_connector_sdk::{ConnectorError, FieldSpec, OptionItem};
use weft_domain::ids::OrgId;

use super::link::{connection_id, open};
use crate::Deps;

/// Options for `core.sync` fields.
pub async fn options(
    deps: &Deps,
    org: OrgId,
    source: &str,
    input: &Value,
) -> Result<Vec<OptionItem>, ConnectorError> {
    match source {
        "connections" => {
            let conns = deps
                .store
                .list_connections(org)
                .await
                .map_err(|e| ConnectorError::transient(e.to_string()))?;
            Ok(conns
                .into_iter()
                .filter(|c| {
                    deps.connectors
                        .get(&c.connector)
                        .is_some_and(|k| k.records().is_some())
                })
                .map(|c| {
                    let app = deps
                        .connectors
                        .get(&c.connector)
                        .map_or_else(|| c.connector.clone(), |k| k.manifest().name.clone());
                    OptionItem::new(c.id.to_string(), format!("{} ({app})", c.name))
                })
                .collect())
        }
        "source_objects" | "target_objects" => {
            let key = if source == "source_objects" {
                "source_connection_id"
            } else {
                "target_connection_id"
            };
            let (connector, ctx) = open(deps, org, connection_id(input, key)?).await?;
            let records = connector
                .records()
                .ok_or_else(|| ConnectorError::invalid("this app does not support record sync"))?;
            Ok(records
                .list_objects(&ctx)
                .await?
                .into_iter()
                .map(|o| OptionItem::new(o.key, o.label))
                .collect())
        }
        other => Err(ConnectorError::unsupported(format!("options `{other}`"))),
    }
}

/// Dynamic sub-fields for `core.sync` (`target_fields`: the target module's writable fields).
pub async fn fields(
    deps: &Deps,
    org: OrgId,
    source: &str,
    input: &Value,
) -> Result<Vec<FieldSpec>, ConnectorError> {
    if source != "target_fields" {
        return Err(ConnectorError::unsupported(format!("fields `{source}`")));
    }
    let (connector, ctx) = open(deps, org, connection_id(input, "target_connection_id")?).await?;
    let records = connector
        .records()
        .ok_or_else(|| ConnectorError::invalid("this app does not support record sync"))?;
    let object = super::link::text(input, "target_object")?;
    Ok(records
        .describe(&ctx, &object)
        .await?
        .into_iter()
        .filter(|f| !f.read_only)
        .collect())
}
