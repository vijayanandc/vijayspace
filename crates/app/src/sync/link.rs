//! Link lookups and explicit link saving; shared helpers for the sync actions.

use std::sync::Arc;

use serde_json::{Value, json};
use weft_connector_sdk::{ActionCtx, Connector, ConnectorError};
use weft_domain::ids::{ConnectionId, OrgId};

use crate::{
    Deps,
    ports::{RecordLink, RecordSide},
    runner::action::load_connection,
};

pub(crate) fn text(input: &Value, key: &str) -> Result<String, ConnectorError> {
    match input.get(key) {
        Some(Value::String(s)) if !s.trim().is_empty() => Ok(s.trim().to_owned()),
        Some(Value::Number(n)) => Ok(n.to_string()),
        Some(
            Value::String(_) | Value::Null | Value::Bool(_) | Value::Array(_) | Value::Object(_),
        )
        | None => Err(ConnectorError::invalid_field(
            key,
            format!("`{key}` is required"),
        )),
    }
}

pub(crate) fn connection_id(input: &Value, key: &str) -> Result<ConnectionId, ConnectorError> {
    text(input, key)?
        .parse()
        .map_err(|_| ConnectorError::invalid_field(key, "Choose a connection"))
}

/// The connector behind a connection plus a context for calling it.
pub(crate) async fn open(
    deps: &Deps,
    org: OrgId,
    id: ConnectionId,
    class: crate::limits::RequestClass,
) -> Result<(Arc<dyn Connector>, ActionCtx), ConnectorError> {
    let handle = load_connection(deps, org, Some(&id.to_string()))
        .await?
        .ok_or_else(|| ConnectorError::invalid("connection not found"))?;
    let stored = deps
        .store
        .get_connection(org, id)
        .await
        .map_err(|e| ConnectorError::transient(e.to_string()))?
        .ok_or_else(|| ConnectorError::invalid("connection not found"))?;
    let connector = deps
        .connectors
        .get(&stored.connection.connector)
        .cloned()
        .ok_or_else(|| ConnectorError::invalid("unknown app"))?;
    Ok((
        connector,
        crate::runner::action::action_ctx_for(deps, org, Some(handle), String::new(), class),
    ))
}

fn side(input: &Value, prefix: &str) -> Result<RecordSide, ConnectorError> {
    Ok(RecordSide {
        connection_id: connection_id(input, &format!("{prefix}_connection_id"))?,
        object: text(input, &format!("{prefix}_object"))?,
        record_id: text(input, &format!("{prefix}_record_id"))?,
    })
}

fn store_err(e: &crate::AppError) -> ConnectorError {
    ConnectorError::transient(e.to_string())
}

/// `resolve_link` (A → B) or, with `reverse`, `find_linked_record` (B → A).
pub(crate) async fn resolve_link(
    deps: &Deps,
    org: OrgId,
    input: &Value,
    reverse: bool,
) -> Result<Value, ConnectorError> {
    let set = text(input, "link_set")?;
    if reverse {
        let b = side(input, "target")?;
        let link = deps
            .store
            .find_link_by_b(org, &set, &b)
            .await
            .map_err(|e| store_err(&e))?;
        Ok(match link {
            Some(l) => {
                json!({"found": true, "source_id": l.a.record_id, "source_object": l.a.object, "source_connection_id": l.a.connection_id})
            }
            None => json!({"found": false, "source_id": null}),
        })
    } else {
        let a = side(input, "source")?;
        let link = deps
            .store
            .find_link_by_a(org, &set, &a)
            .await
            .map_err(|e| store_err(&e))?;
        Ok(match link {
            Some(l) => {
                json!({"found": true, "target_id": l.b.record_id, "target_object": l.b.object, "target_connection_id": l.b.connection_id})
            }
            None => json!({"found": false, "target_id": null}),
        })
    }
}

/// `save_link`: records a pairing explicitly.
pub(crate) async fn save_link(
    deps: &Deps,
    org: OrgId,
    input: &Value,
) -> Result<Value, ConnectorError> {
    let link = RecordLink {
        id: deps.new_link_id(),
        org_id: org,
        link_set: text(input, "link_set")?,
        a: side(input, "source")?,
        b: side(input, "target")?,
        b_hash: None,
        b_written_at: None,
    };
    deps.store
        .upsert_link(&link, deps.clock.now())
        .await
        .map_err(|e| ConnectorError::invalid(e.message))?;
    Ok(json!({"saved": true}))
}
