//! `JobQueue` for MySQL: claims with `FOR UPDATE SKIP LOCKED`, leases, reaping, dead letters.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{FromRow, QueryBuilder, types::Json};
use uuid::Uuid;
use weft_app::ports::{ClaimedJob, JobQueue, NewJob, Res};
use weft_domain::ids::{ExecutionId, JobId};

use crate::{
    MySqlStore,
    codec::{db_err, is_duplicate},
};

#[derive(Debug, FromRow)]
struct JobRow {
    id: Uuid,
    queue: String,
    kind: String,
    payload: Json<Value>,
    attempts: u32,
    max_attempts: u32,
}

pub(crate) async fn insert_job<'e, E: sqlx::MySqlExecutor<'e>>(
    exec: E,
    job: &NewJob,
    now: DateTime<Utc>,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("INSERT INTO jobs (id, queue, kind, org_id, payload, status, run_at, dedupe_key, created_at, updated_at) VALUES (?, ?, ?, ?, ?, 'ready', ?, ?, ?, ?)")
        .bind(job.id.as_uuid())
        .bind(&job.queue)
        .bind(&job.kind)
        .bind(job.org_id.map(|o| o.as_uuid()))
        .bind(Json(&job.payload))
        .bind(job.run_at)
        .bind(&job.dedupe_key)
        .bind(now)
        .bind(now)
        .execute(exec)
        .await;
    match result {
        Ok(_) => Ok(true),
        Err(e) if is_duplicate(&e) => Ok(false),
        Err(e) => Err(e),
    }
}

#[async_trait]
impl JobQueue for MySqlStore {
    async fn enqueue(&self, job: &NewJob, now: DateTime<Utc>) -> Res<bool> {
        insert_job(&self.pool, job, now).await.map_err(db_err)
    }

    async fn claim(
        &self,
        queues: &[&str],
        worker: &str,
        limit: u32,
        now: DateTime<Utc>,
        lease_until: DateTime<Utc>,
    ) -> Res<Vec<ClaimedJob>> {
        if queues.is_empty() {
            return Ok(Vec::new());
        }
        let mut tx = self.pool.begin().await.map_err(db_err)?;
        let mut qb = QueryBuilder::new(
            "SELECT id, queue, kind, payload, attempts, max_attempts FROM jobs WHERE status = 'ready' AND run_at <= ",
        );
        qb.push_bind(now).push(" AND queue IN (");
        let mut sep = qb.separated(", ");
        for q in queues {
            sep.push_bind(*q);
        }
        qb.push(") ORDER BY run_at LIMIT ")
            .push_bind(limit)
            .push(" FOR UPDATE SKIP LOCKED");
        let rows: Vec<JobRow> = qb
            .build_query_as()
            .fetch_all(&mut *tx)
            .await
            .map_err(db_err)?;
        for row in &rows {
            sqlx::query("UPDATE jobs SET status = 'running', locked_by = ?, locked_until = ?, attempts = attempts + 1, updated_at = ? WHERE id = ?")
                .bind(worker)
                .bind(lease_until)
                .bind(now)
                .bind(row.id)
                .execute(&mut *tx)
                .await
                .map_err(db_err)?;
        }
        tx.commit().await.map_err(db_err)?;
        Ok(rows
            .into_iter()
            .map(|r| ClaimedJob {
                id: JobId::from_uuid(r.id),
                queue: r.queue,
                kind: r.kind,
                payload: r.payload.0,
                attempts: r.attempts + 1,
                max_attempts: r.max_attempts,
            })
            .collect())
    }

    async fn complete(&self, id: JobId) -> Res<()> {
        sqlx::query("DELETE FROM jobs WHERE id = ?")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(())
    }

    async fn reschedule(&self, id: JobId, run_at: DateTime<Utc>, now: DateTime<Utc>) -> Res<()> {
        // A deliberate reschedule (delay, retry backoff) is not a failure: reset the attempt count.
        sqlx::query("UPDATE jobs SET status = 'ready', run_at = ?, locked_by = NULL, locked_until = NULL, attempts = 0, updated_at = ? WHERE id = ?")
            .bind(run_at)
            .bind(now)
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(())
    }

    async fn fail(
        &self,
        id: JobId,
        error: &str,
        retry_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Res<bool> {
        let result = sqlx::query(
            "UPDATE jobs SET status = IF(attempts >= max_attempts, 'dead', 'ready'), run_at = ?, last_error = ?, locked_by = NULL, locked_until = NULL, updated_at = ? WHERE id = ?",
        )
        .bind(retry_at)
        .bind(error.chars().take(4000).collect::<String>())
        .bind(now)
        .bind(id.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        let dead: Option<String> = sqlx::query_scalar("SELECT status FROM jobs WHERE id = ?")
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(result.rows_affected() > 0 && dead.as_deref() == Some("dead"))
    }

    async fn heartbeat(&self, id: JobId, lease_until: DateTime<Utc>) -> Res<()> {
        sqlx::query("UPDATE jobs SET locked_until = ? WHERE id = ? AND status = 'running'")
            .bind(lease_until)
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(())
    }

    async fn reap(&self, now: DateTime<Utc>) -> Res<u64> {
        let result = sqlx::query("UPDATE jobs SET status = 'ready', locked_by = NULL, locked_until = NULL, updated_at = ? WHERE status = 'running' AND locked_until < ?")
            .bind(now)
            .bind(now)
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(result.rows_affected())
    }

    async fn delete_jobs_for_execution(&self, execution: ExecutionId) -> Res<()> {
        sqlx::query("DELETE FROM jobs WHERE dedupe_key = ? AND status <> 'running'")
            .bind(format!("exe:{execution}"))
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(())
    }
}
