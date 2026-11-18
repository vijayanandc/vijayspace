//! `upsert_linked_record`: the sync workhorse (doc 13 §4.2).
//!
//! lease → lookups → hash → link? (unchanged → skip | update) : (match → update | create) → save link.

use chrono::Duration;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use weft_connector_sdk::{ConnectorError, RecordObjects};
use weft_domain::{error_codes, ids::OrgId};

use super::link::{connection_id, open, text};
use super::target::{TargetRef, busy, claim_target};
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
/// How long one sync of a record holds its lease. It must outlast the slowest path
/// (update → not found → find → create, each up to a 30 s call, plus lookups), or a second
/// sync of the same record could start and create a duplicate.
pub(super) const LEASE_SECS: i64 = 300;

pub(crate) async fn upsert_linked_record(
    deps: &Deps,
    org: OrgId,
    input: &Value,
    class: crate::limits::RequestClass,
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
        .acquire_lease(
            org,
            &lease_key,
            &holder,
            now + Duration::seconds(LEASE_SECS),
            now,
        )
        .await
        .map_err(|e| ConnectorError::transient(e.to_string()))?;
    if !leased {
        return Err(busy());
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
        class,
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
    class: crate::limits::RequestClass,
) -> Result<Value, ConnectorError> {
    apply_lookups(deps, org, input, values).await?;
    let hash = values_hash(values);
    let (connector, ctx) = open(deps, org, target_conn, class).await?;
    let target = records(connector.as_ref())?;
    let link = deps
        .store
        .find_link_by_a(org, set, source)
        .await
        .map_err(|e| ConnectorError::transient(e.to_string()))?;
    // A link that points at another connection or module (the flow was re-pointed) is stale:
    // never write the old record ID into the new target.
    let link = link.filter(|l| l.b.connection_id == target_conn && l.b.object == target_object);
    if let Some(l) = &link
        && l.b_hash.as_deref() == Some(hash.as_slice())
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
    // A record found through the match field is leased like the source record, so two source
    // records matching the same target cannot both write it before one of them is linked. The
    // lease is released on every path, errors included.
    let target_holder = deps.new_link_id().to_string();
    let claimed: std::sync::Mutex<Option<Vec<u8>>> = std::sync::Mutex::new(None);
    let outcome = async {
        let written = if let Some(w) = written {
            w
        } else {
            let claim = |candidate: String| {
                let target = TargetRef {
                    org,
                    set,
                    conn: target_conn,
                    object: target_object,
                };
                claim_target(deps, target, source, &target_holder, &claimed, candidate)
            };
            create_or_recover(target, &ctx, input, target_object, values, claim).await?
        };
        let (target_id, action) = written;
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
            .map_err(|e| link_save_error(&e))?;
        Ok(json!({"action": action, "target_id": target_id, "link_set": set}))
    }
    .await;
    let key = claimed.lock().ok().and_then(|mut slot| slot.take());
    if let Some(key) = key {
        let _ = deps.store.release_lease(org, &key, &target_holder).await;
    }
    outcome
}

/// A conflict (side B taken by another source record) is permanent; anything else may pass.
fn link_save_error(e: &crate::AppError) -> ConnectorError {
    match e.kind {
        crate::ErrorKind::Conflict | crate::ErrorKind::Forbidden => {
            ConnectorError::invalid(e.message.clone())
        }
        crate::ErrorKind::Validation
        | crate::ErrorKind::NotFound
        | crate::ErrorKind::Unauthorized
        | crate::ErrorKind::BadGateway
        | crate::ErrorKind::Internal => ConnectorError::transient(e.message.clone()),
    }
}

/// `RecoverCreate`: search by the match field first, so a crash after "create" never duplicates.
/// `linked_elsewhere(id)` tells whether a matched record already belongs to another source record
/// in this link set; such a match is refused before anything is written.
async fn create_or_recover<F, Fut>(
    target: &dyn RecordObjects,
    ctx: &weft_connector_sdk::ActionCtx,
    input: &Value,
    object: &str,
    values: &Map<String, Value>,
    linked_elsewhere: F,
) -> Result<(String, &'static str), ConnectorError>
where
    F: FnOnce(String) -> Fut,
    Fut: std::future::Future<Output = Result<bool, ConnectorError>>,
{
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
                if linked_elsewhere(id.clone()).await? {
                    return Err(ConnectorError::invalid(format!(
                        "{}: the record matching {field} = {value} is already linked to another source record",
                        error_codes::SYNC_AMBIGUOUS_MATCH
                    )));
                }
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
