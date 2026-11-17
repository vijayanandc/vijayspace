//! Trigger-event inserts used inside the execution-creation transaction (dedupe, debounce).

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{MySql, Transaction, types::Json};
use uuid::Uuid;
use weft_app::ports::{NewExecution, NewTriggerEvent, Res};
use weft_domain::ids::{ExecutionId, OrgId};

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

/// Debounce: gives the burst's execution the newest trigger data while its job is still waiting.
/// The job row is locked first, so a worker cannot claim it until this commits; a job that is
/// already claimed may have read the old data, so the caller starts a new execution instead.
pub(crate) async fn coalesce_into_queued(
    tx: &mut Transaction<'_, MySql>,
    org: OrgId,
    ev: &NewTriggerEvent,
    trigger_data: &Value,
) -> Res<bool> {
    let execution: Option<Uuid> = sqlx::query_scalar("SELECT e.id FROM executions e JOIN trigger_events t ON t.id = e.trigger_event_id WHERE e.org_id = ? AND t.registration_id = ? AND t.dedupe_key = ? AND e.status = 'queued' AND e.started_at IS NULL FOR UPDATE")
        .bind(org.as_uuid())
        .bind(ev.registration_id.as_uuid())
        .bind(&ev.dedupe_key)
        .fetch_optional(&mut **tx)
        .await
        .map_err(db_err)?;
    let Some(execution) = execution else {
        return Ok(false);
    };
    let job_key = format!("exe:{}", ExecutionId::from_uuid(execution));
    let waiting: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM jobs WHERE dedupe_key = ? AND status = 'ready' FOR UPDATE",
    )
    .bind(&job_key)
    .fetch_optional(&mut **tx)
    .await
    .map_err(db_err)?;
    if waiting.is_none() {
        return Ok(false);
    }
    sqlx::query("UPDATE executions SET trigger_data = ? WHERE id = ?")
        .bind(Json(trigger_data))
        .bind(execution)
        .execute(&mut **tx)
        .await
        .map_err(db_err)?;
    Ok(true)
}
