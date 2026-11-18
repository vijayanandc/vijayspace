//! Creating executions from trigger events in one transaction: dedupe, debounce coalescing.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{MySql, Transaction, types::Json};
use uuid::Uuid;
use weft_app::ports::{NewExecution, NewTriggerEvent, Res};
use weft_domain::ids::{ExecutionId, OrgId};

use crate::{
    MySqlStore,
    codec::{db_err, is_duplicate},
    executions::insert_execution,
    jobs::insert_job,
    usage::bump_usage,
};

/// Inserts a trigger event; `Ok(false)` when its dedupe key already exists for the registration
/// (`InnoDB` rolls back only the failed statement, so the transaction continues). Dedupe keys live in
/// `trigger_dedupe` (not partitioned), so a key is recognised again on any later day within 30 days.
pub(crate) async fn insert_event(
    tx: &mut Transaction<'_, MySql>,
    org: OrgId,
    item: &NewExecution,
    ev: &NewTriggerEvent,
    now: DateTime<Utc>,
) -> Res<bool> {
    if let Some(key) = &ev.dedupe_key {
        let claimed = sqlx::query(
            "INSERT INTO trigger_dedupe (registration_id, dedupe_key, event_id, created_at) VALUES (?, ?, ?, ?)",
        )
        .bind(ev.registration_id.as_uuid())
        .bind(key)
        .bind(ev.id.as_uuid())
        .bind(now)
        .execute(&mut **tx)
        .await;
        match claimed {
            Ok(_) => {}
            Err(e) if is_duplicate(&e) => return Ok(false),
            Err(e) => return Err(db_err(e)),
        }
    }
    let inserted = sqlx::query("INSERT INTO trigger_events (id, org_id, flow_id, registration_id, dedupe_key, status, payload, received_at, day) VALUES (?, ?, ?, ?, ?, 'accepted', ?, ?, ?)")
        .bind(ev.id.as_uuid())
        .bind(org.as_uuid())
        .bind(item.execution.flow_id.as_uuid())
        .bind(ev.registration_id.as_uuid())
        .bind(&ev.dedupe_key)
        .bind(Json(&ev.payload))
        .bind(now)
        .bind(now.date_naive())
        .execute(&mut **tx)
        .await;
    inserted.map(|_| true).map_err(db_err)
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
    // Find the burst's execution with a plain read: locking through `trigger_events` would take
    // an exclusive lock on the event row that every concurrent delivery of the burst already
    // holds a shared lock on (from its duplicate insert), which deadlocks.
    let found: Option<Uuid> = sqlx::query_scalar("SELECT e.id FROM executions e JOIN trigger_dedupe d ON d.event_id = e.trigger_event_id WHERE e.org_id = ? AND d.registration_id = ? AND d.dedupe_key = ?")
        .bind(org.as_uuid())
        .bind(ev.registration_id.as_uuid())
        .bind(&ev.dedupe_key)
        .fetch_optional(&mut **tx)
        .await
        .map_err(db_err)?;
    let Some(found) = found else {
        return Ok(false);
    };
    // Then lock only the execution and its job (the same order as requeueing).
    let execution: Option<Uuid> = sqlx::query_scalar("SELECT id FROM executions WHERE id = ? AND status = 'queued' AND started_at IS NULL FOR UPDATE")
        .bind(found)
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

impl MySqlStore {
    /// One attempt of `ExecutionRepo::create_executions`.
    pub(crate) async fn create_executions_once(
        &self,
        org: OrgId,
        items: &[NewExecution],
        now: DateTime<Utc>,
    ) -> Res<Vec<ExecutionId>> {
        let mut tx = self.pool.begin().await.map_err(db_err)?;
        let mut created = Vec::with_capacity(items.len());
        for item in items {
            if let Some(seen) = &item.seen
                && !insert_event(&mut tx, org, item, seen, now).await?
            {
                continue; // this exact version was already delivered
            }
            if let Some(ev) = &item.event
                && !insert_event(&mut tx, org, item, ev, now).await?
            {
                if !item.coalesce {
                    continue; // already seen (dedupe)
                }
                if coalesce_into_queued(&mut tx, org, ev, &item.execution.trigger_data).await? {
                    continue; // the burst's queued execution now carries the newest data
                }
                // The burst's execution already started: run this event on its own (the event
                // keeps its ID, so the execution's `trigger_event_id` stays valid).
                let unkeyed = NewTriggerEvent {
                    dedupe_key: None,
                    ..ev.clone()
                };
                insert_event(&mut tx, org, item, &unkeyed, now).await?;
            }
            insert_execution(&mut tx, &item.execution, now)
                .await
                .map_err(db_err)?;
            insert_job(&mut *tx, &item.job, now).await.map_err(db_err)?;
            created.push(item.execution.id);
        }
        if !created.is_empty() {
            bump_usage(
                &mut tx,
                org,
                now,
                0,
                u32::try_from(created.len()).unwrap_or(u32::MAX),
            )
            .await
            .map_err(db_err)?;
        }
        tx.commit().await.map_err(db_err)?;
        Ok(created)
    }
}
