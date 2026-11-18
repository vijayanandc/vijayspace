//! `RetentionRepo` for MySQL: short batched deletes (no long locks), oldest first.

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::QueryBuilder;
use uuid::Uuid;
use weft_app::ports::{Housekeeping, PartitionReport, Purged, Res, RetentionRepo};
use weft_domain::ids::OrgId;

use crate::{MySqlStore, codec::db_err};

/// Step runs are deleted in chunks of this many rows (they can hold large JSON).
const STEP_CHUNK: u32 = 500;

fn id_list<'a>(qb: &mut QueryBuilder<'a, sqlx::MySql>, ids: &'a [Uuid]) {
    qb.push(" (");
    let mut sep = qb.separated(", ");
    for id in ids {
        sep.push_bind(*id);
    }
    qb.push(")");
}

impl MySqlStore {
    /// Finished executions (and their step runs) that finished before `before`. Each statement
    /// is short on its own: step runs go in bounded chunks first, then the executions. A crash in
    /// between leaves executions without steps, which the next sweep removes.
    async fn purge_executions(
        &self,
        org: OrgId,
        before: DateTime<Utc>,
        limit: u32,
    ) -> Res<(u64, u64)> {
        let ids: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM executions WHERE org_id = ? AND finished_at IS NOT NULL AND finished_at < ? AND status IN ('succeeded', 'failed', 'cancelled') ORDER BY finished_at LIMIT ?")
            .bind(org.as_uuid())
            .bind(before)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(db_err)?;
        if ids.is_empty() {
            return Ok((0, 0));
        }
        let mut step_runs = 0;
        loop {
            // Re-checked per chunk: a run retried since it was selected keeps its history. The id
            // list stays on the outer query so MySQL uses the `execution_id` index (a LIMIT keeps
            // a single-table DELETE from turning `IN (SELECT …)` into a semijoin: it would scan).
            let mut qb = QueryBuilder::new("DELETE FROM step_runs WHERE org_id = ");
            qb.push_bind(org.as_uuid()).push(" AND execution_id IN");
            id_list(&mut qb, &ids);
            qb.push(" AND EXISTS (SELECT 1 FROM executions e WHERE e.id = step_runs.execution_id AND e.org_id = ")
                .push_bind(org.as_uuid())
                .push(" AND e.status IN ('succeeded', 'failed', 'cancelled'))");
            qb.push(" LIMIT ").push_bind(STEP_CHUNK);
            let n = qb
                .build()
                .execute(&self.pool)
                .await
                .map_err(db_err)?
                .rows_affected();
            step_runs += n;
            if n < u64::from(STEP_CHUNK) {
                break;
            }
        }
        let mut qb = QueryBuilder::new("DELETE FROM executions WHERE org_id = ");
        qb.push_bind(org.as_uuid())
            .push(" AND status IN ('succeeded', 'failed', 'cancelled') AND id IN");
        id_list(&mut qb, &ids);
        let executions = qb
            .build()
            .execute(&self.pool)
            .await
            .map_err(db_err)?
            .rows_affected();
        Ok((executions, step_runs))
    }
}

#[async_trait]
impl RetentionRepo for MySqlStore {
    async fn purge_history(&self, org: OrgId, before: DateTime<Utc>, limit: u32) -> Res<Purged> {
        let (executions, step_runs) = self.purge_executions(org, before, limit).await?;
        // Events an execution still points to stay (they are its trigger data's source).
        let trigger_events = sqlx::query("DELETE FROM trigger_events WHERE org_id = ? AND received_at < ? AND NOT EXISTS (SELECT 1 FROM executions e WHERE e.trigger_event_id = trigger_events.id) ORDER BY received_at LIMIT ?")
            .bind(org.as_uuid())
            .bind(before)
            .bind(limit)
            .execute(&self.pool)
            .await
            .map_err(db_err)?
            .rows_affected();
        let test_snapshots = sqlx::query("DELETE FROM flow_versions WHERE org_id = ? AND kind = 'test_snapshot' AND created_at < ? AND NOT EXISTS (SELECT 1 FROM executions e WHERE e.flow_version_id = flow_versions.id) ORDER BY created_at LIMIT ?")
            .bind(org.as_uuid())
            .bind(before)
            .bind(limit)
            .execute(&self.pool)
            .await
            .map_err(db_err)?
            .rows_affected();
        Ok(Purged {
            executions,
            step_runs,
            trigger_events,
            test_snapshots,
        })
    }

    async fn housekeeping(
        &self,
        jobs_before: DateTime<Utc>,
        now: DateTime<Utc>,
        usage_before: NaiveDate,
        history_before: DateTime<Utc>,
        limit: u32,
    ) -> Res<Housekeeping> {
        let run = |sql: &'static str| sqlx::query(sql);
        let dead_jobs = run(
            "DELETE FROM jobs WHERE status = 'dead' AND updated_at < ? ORDER BY updated_at LIMIT ?",
        )
        .bind(jobs_before)
        .bind(limit)
        .execute(&self.pool)
        .await
        .map_err(db_err)?
        .rows_affected();
        let oauth_states = run("DELETE FROM oauth_states WHERE expires_at < ? LIMIT ?")
            .bind(now)
            .bind(limit)
            .execute(&self.pool)
            .await
            .map_err(db_err)?
            .rows_affected();
        let sync_leases = run("DELETE FROM sync_leases WHERE locked_until < ? LIMIT ?")
            .bind(now)
            .bind(limit)
            .execute(&self.pool)
            .await
            .map_err(db_err)?
            .rows_affected();
        let usage_days = run("DELETE FROM connection_usage WHERE day < ? LIMIT ?")
            .bind(usage_before)
            .bind(limit)
            .execute(&self.pool)
            .await
            .map_err(db_err)?
            .rows_affected()
            + run("DELETE FROM send_counters WHERE day < ? LIMIT ?")
                .bind(usage_before)
                .bind(limit)
                .execute(&self.pool)
                .await
                .map_err(db_err)?
                .rows_affected();
        // Kept a day after expiring or being revoked: they count toward the org's daily emails.
        let invitations = run("DELETE FROM org_invitations WHERE expires_at < ? LIMIT ?")
            .bind(now - chrono::Duration::days(1))
            .bind(limit)
            .execute(&self.pool)
            .await
            .map_err(db_err)?
            .rows_affected();
        let dedupe_keys = run("DELETE FROM trigger_dedupe WHERE created_at < ? LIMIT ?")
            .bind(history_before)
            .bind(limit)
            .execute(&self.pool)
            .await
            .map_err(db_err)?
            .rows_affected();
        let test_snapshots = run(
            "DELETE FROM flow_versions WHERE kind = 'test_snapshot' AND created_at < ? \
             AND NOT EXISTS (SELECT 1 FROM executions e WHERE e.flow_version_id = flow_versions.id) LIMIT ?",
        )
        .bind(history_before)
        .bind(limit)
        .execute(&self.pool)
        .await
        .map_err(db_err)?
        .rows_affected();
        Ok(Housekeeping {
            dedupe_keys,
            test_snapshots,
            invitations,
            dead_jobs,
            oauth_states,
            sync_leases,
            usage_days,
        })
    }

    async fn maintain_partitions(
        &self,
        today: NaiveDate,
        keep_days: u32,
        ahead_days: u32,
        now: DateTime<Utc>,
    ) -> Res<PartitionReport> {
        self.maintain(today, keep_days, ahead_days, now).await
    }
}
