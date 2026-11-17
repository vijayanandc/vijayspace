//! Trigger-event inserts used inside the execution-creation transaction (dedupe, debounce).

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{MySql, Transaction, types::Json};
use weft_app::ports::{NewExecution, NewTriggerEvent, Res};
use weft_domain::ids::OrgId;

use crate::codec::{db_err, is_duplicate};

/// Inserts a trigger event; `Ok(false)` when its dedupe key already exists for the registration
/// (`InnoDB` rolls back only the failed statement, so the transaction continues).
pub(crate) async fn insert_event(
    tx: &mut Transaction<'_, MySql>,
    org: OrgId,
    item: &NewExecution,
    ev: &NewTriggerEvent,
    now: DateTime<Utc>,
) -> Res<bool> {
    let inserted = sqlx::query("INSERT INTO trigger_events (id, org_id, flow_id, registration_id, dedupe_key, status, payload, received_at) VALUES (?, ?, ?, ?, ?, 'accepted', ?, ?)")
        .bind(ev.id.as_uuid())
        .bind(org.as_uuid())
        .bind(item.execution.flow_id.as_uuid())
        .bind(ev.registration_id.as_uuid())
        .bind(&ev.dedupe_key)
        .bind(Json(&ev.payload))
        .bind(now)
        .execute(&mut **tx)
        .await;
    match inserted {
        Ok(_) => Ok(true),
        Err(e) if is_duplicate(&e) => Ok(false),
        Err(e) => Err(db_err(e)),
    }
}

/// Debounce: gives the burst's execution the newest trigger data while it is still queued.
pub(crate) async fn coalesce_into_queued(
    tx: &mut Transaction<'_, MySql>,
    org: OrgId,
    ev: &NewTriggerEvent,
    trigger_data: &Value,
) -> Res<bool> {
    let updated = sqlx::query("UPDATE executions e JOIN trigger_events t ON t.id = e.trigger_event_id SET e.trigger_data = ? WHERE e.org_id = ? AND t.registration_id = ? AND t.dedupe_key = ? AND e.status = 'queued' AND e.started_at IS NULL")
        .bind(Json(trigger_data))
        .bind(org.as_uuid())
        .bind(ev.registration_id.as_uuid())
        .bind(&ev.dedupe_key)
        .execute(&mut **tx)
        .await
        .map_err(db_err)?;
    Ok(updated.rows_affected() > 0)
}
