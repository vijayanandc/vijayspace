//! `upsert_linked_record`: the sync workhorse (doc 13 §4.2).
//!
//! lease → lookups → hash → link? (unchanged → skip | update) : (match → update | create) → save link.

use chrono::Duration;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use weft_connector_sdk::{ConnectorError, RecordObjects};
use weft_domain::{error_codes, ids::OrgId};

use super::link::{connection_id, open, text};
use crate::{
    Deps,
    ports::{RecordLink, RecordSide},
};

/// Sorts object keys recursively so equal values always hash the same.
fn canonical(v: &Value) -> Value {
    match v {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            Value::Object(
                keys.into_iter()
                    .map(|k| (k.clone(), canonical(&map[k])))
                    .collect(),
            )
        }
        Value::Array(items) => Value::Array(items.iter().map(canonical).collect()),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => v.clone(),
    }
}

/// SHA-256 of the canonical JSON of `values`.
#[must_use]
pub fn values_hash(values: &Map<String, Value>) -> Vec<u8> {
    Sha256::digest(
        canonical(&Value::Object(values.clone()))
            .to_string()
            .as_bytes(),
    )
    .to_vec()
}

fn records(c: &dyn weft_connector_sdk::Connector) -> Result<&dyn RecordObjects, ConnectorError> {
    c.records().ok_or_else(|| {
        ConnectorError::invalid(format!(
            "{} does not support record sync",
            c.manifest().name
        ))
    })
}

/// Resolves `lookups: [{field, link_set, source_record_id, source_connection_id?, source_object?}]`.
async fn apply_lookups(
    deps: &Deps,
    org: OrgId,
    input: &Value,
    values: &mut Map<String, Value>,
) -> Result<(), ConnectorError> {
    let lookups = match input.get("lookups") {
        Some(Value::Array(items)) => items.clone(),
        Some(Value::String(s)) if !s.trim().is_empty() => serde_json::from_str::<Vec<Value>>(s)
            .map_err(|e| ConnectorError::invalid_field("lookups", e.to_string()))?,
        Some(
            Value::String(_) | Value::Null | Value::Bool(_) | Value::Number(_) | Value::Object(_),
        )
        | None => Vec::new(),
    };
    for lk in lookups {
        let field = text(&lk, "field")?;
        let Ok(source_id) = text(&lk, "source_record_id") else {
            continue; // empty lookup source (e.g. no account on the contact): leave the field unset
        };
        let set = text(&lk, "link_set")?;
        let conn = lk.get("source_connection_id").map_or_else(
            || connection_id(input, "source_connection_id"),
            |_| connection_id(&lk, "source_connection_id"),
        )?;
        let object = text(&lk, "source_object").map_err(|_| {
            ConnectorError::invalid_field("lookups", "each lookup needs `source_object`")
        })?;
        let a = RecordSide {
            connection_id: conn,
            object,
            record_id: source_id.clone(),
        };
        let link = deps
            .store
            .find_link_by_a(org, &set, &a)
            .await
            .map_err(|e| ConnectorError::transient(e.to_string()))?;
        match link {
            Some(l) => {
                values.insert(field, Value::String(l.b.record_id));
            }
            // The parent record may not be synced yet (events can arrive out of order): retry later.
            None => {
                return Err(ConnectorError::RateLimited {
                    retry_after_secs: Some(60),
                    message: format!(
                        "{}: `{source_id}` is not linked yet in `{set}`",
                        error_codes::SYNC_LOOKUP_MISSING
                    ),
                });
            }
        }
    }
    Ok(())
}

/// Creates or updates the target record linked to the source record.
pub(crate) async fn upsert_linked_record(
    deps: &Deps,
    org: OrgId,
    input: &Value,
) -> Result<Value, ConnectorError> {
    let set = text(input, "link_set")?;
    let source = RecordSide {
        connection_id: connection_id(input, "source_connection_id")?,
        object: text(input, "source_object")?,
        record_id: text(input, "source_record_id")?,
    };
    let target_conn = connection_id(input, "target_connection_id")?;
    let target_object = text(input, "target_object")?;
    let mut values = input
        .get("values")
        .and_then(Value::as_object)
        .cloned()
        .ok_or_else(|| ConnectorError::invalid_field("values", "Map at least one target field"))?;
    values.retain(|_, v| !v.is_null());

    let lease_key = Sha256::digest(
        format!(
            "{set}|{}|{}|{}",
            source.connection_id, source.object, source.record_id
        )
        .as_bytes(),
    )
    .to_vec();
    let holder = deps.new_link_id().to_string();
    let now = deps.clock.now();
    let leased = deps
        .store
        .acquire_lease(org, &lease_key, &holder, now + Duration::seconds(60), now)
        .await
        .map_err(|e| ConnectorError::transient(e.to_string()))?;
    if !leased {
        return Err(ConnectorError::RateLimited {
            retry_after_secs: Some(10),
            message: format!(
                "{}: another sync of this record is running",
                error_codes::SYNC_BUSY
            ),
        });
    }
    let result = upsert_locked(
        deps,
        org,
        input,
        &set,
        &source,
        target_conn,
        &target_object,
        &mut values,
    )
    .await;
    let _ = deps.store.release_lease(org, &lease_key, &holder).await;
    result
}

#[allow(clippy::too_many_arguments)]
async fn upsert_locked(
    deps: &Deps,
    org: OrgId,
    input: &Value,
    set: &str,
    source: &RecordSide,
    target_conn: weft_domain::ids::ConnectionId,
    target_object: &str,
    values: &mut Map<String, Value>,
) -> Result<Value, ConnectorError> {
    apply_lookups(deps, org, input, values).await?;
    let hash = values_hash(values);
    let (connector, ctx) = open(deps, org, target_conn).await?;
    let target = records(connector.as_ref())?;
    let link = deps
        .store
        .find_link_by_a(org, set, source)
        .await
        .map_err(|e| ConnectorError::transient(e.to_string()))?;
    if let Some(l) = &link
        && l.b_hash.as_deref() == Some(hash.as_slice())
        && l.b.object == target_object
    {
        return Ok(
            json!({"action": "skipped", "reason": "unchanged", "target_id": l.b.record_id, "link_set": set}),
        );
    }
    let mut written: Option<(String, &'static str)> = None;
    if let Some(l) = &link {
        match target
            .update(&ctx, target_object, &l.b.record_id, values)
            .await
        {
            Ok(r) => written = Some((r.id, "updated")),
            Err(ConnectorError::NotFound { .. }) => {
                // Stale link: the target record was deleted. Forget the link and recreate.
                deps.store
                    .delete_link(l.id)
                    .await
                    .map_err(|e| ConnectorError::transient(e.to_string()))?;
            }
            Err(e) => return Err(e),
        }
    }
    if written.is_none() {
        written = Some(create_or_recover(target, &ctx, input, target_object, values).await?);
    }
    let (target_id, action) = written.unwrap_or_default();
    let new_link = RecordLink {
        id: link.as_ref().map_or_else(|| deps.new_link_id(), |l| l.id),
        org_id: org,
        link_set: set.to_owned(),
        a: source.clone(),
        b: RecordSide {
            connection_id: target_conn,
            object: target_object.to_owned(),
            record_id: target_id.clone(),
        },
        b_hash: Some(hash),
        b_written_at: Some(deps.clock.now()),
    };
    deps.store
        .upsert_link(&new_link, deps.clock.now())
        .await
        .map_err(|e| ConnectorError::invalid(e.message))?;
    Ok(json!({"action": action, "target_id": target_id, "link_set": set}))
}

/// `RecoverCreate`: search by the match field first, so a crash after "create" never duplicates.
async fn create_or_recover(
    target: &dyn RecordObjects,
    ctx: &weft_connector_sdk::ActionCtx,
    input: &Value,
    object: &str,
    values: &Map<String, Value>,
) -> Result<(String, &'static str), ConnectorError> {
    let match_field = input
        .get("match_field")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    let match_value = input
        .get("match_value")
        .filter(|v| !v.is_null() && v.as_str() != Some(""));
    if let (Some(field), Some(value)) = (match_field, match_value) {
        let found = target.find_by(ctx, object, field, value).await?;
        match found.len() {
            0 => {}
            1 => {
                let id = found[0].id.clone();
                let r = target.update(ctx, object, &id, values).await?;
                return Ok((r.id, "updated"));
            }
            n => {
                return Err(ConnectorError::invalid(format!(
                    "{}: {n} records match {field} = {value}",
                    error_codes::SYNC_AMBIGUOUS_MATCH
                )));
            }
        }
    }
    let r = target.create(ctx, object, values).await?;
    Ok((r.id, "created"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_ignores_key_order() {
        let a: Map<String, Value> =
            serde_json::from_value(json!({"b": 1, "a": {"y": 2, "x": 1}})).unwrap();
        let b: Map<String, Value> =
            serde_json::from_value(json!({"a": {"x": 1, "y": 2}, "b": 1})).unwrap();
        assert_eq!(values_hash(&a), values_hash(&b));
        let c: Map<String, Value> =
            serde_json::from_value(json!({"a": {"x": 1, "y": 3}, "b": 1})).unwrap();
        assert_ne!(values_hash(&a), values_hash(&c));
    }
}
