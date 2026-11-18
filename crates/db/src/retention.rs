//! `RetentionRepo` for MySQL: short batched deletes (no long locks), oldest first.

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::QueryBuilder;
use uuid::Uuid;
use weft_app::ports::{Housekeeping, Purged, Res, RetentionRepo};
use weft_domain::ids::OrgId;

use crate::{MySqlStore, codec::db_err};

impl MySqlStore {
    /// Finished executions (and their step runs) that finished before `before`.
    async fn purge_executions(
        &self,
        org: OrgId,
        before: DateTime<Utc>,
        limit: u32,
    ) -> Res<(u64, u64)> {
        let mut tx = self.pool.begin().await.map_err(db_err)?;
        let ids: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM executions WHERE org_id = ? AND finished_at IS NOT NULL AND finished_at < ? AND status IN ('succeeded', 'failed', 'cancelled') ORDER BY finished_at LIMIT ? FOR UPDATE")
            .bind(org.as_uuid())
            .bind(before)
            .bind(limit)
            .fetch_all(&mut *tx)
            .await
            .map_err(db_err)?;
        if ids.is_empty() {
            return Ok((0, 0));
        }
        let mut steps = QueryBuilder::new("DELETE FROM step_runs WHERE org_id = ");
        steps
            .push_bind(org.as_uuid())
            .push(" AND execution_id IN (");
        let mut sep = steps.separated(", ");
        for id in &ids {
            sep.push_bind(*id);
        }
        steps.push(")");
        let step_runs = steps
            .build()
            .execute(&mut *tx)
            .await
            .map_err(db_err)?
            .rows_affected();
        let mut exes = QueryBuilder::new("DELETE FROM executions WHERE org_id = ");
        exes.push_bind(org.as_uuid()).push(" AND id IN (");
        let mut sep = exes.separated(", ");
        for id in &ids {
            sep.push_bind(*id);
        }
        exes.push(")");
        let executions = exes
            .build()
            .execute(&mut *tx)
            .await
            .map_err(db_err)?
            .rows_affected();
        tx.commit().await.map_err(db_err)?;
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
            .rows_affected();
        Ok(Housekeeping {
            dead_jobs,
            oauth_states,
            sync_leases,
            usage_days,
        })
    }
}
