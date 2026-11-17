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
                holder: worker.to_owned(),
            })
            .collect())
    }

    async fn complete(&self, job: &ClaimedJob) -> Res<bool> {
        let r =
            sqlx::query("DELETE FROM jobs WHERE id = ? AND status = 'running' AND locked_by = ?")
                .bind(job.id.as_uuid())
                .bind(&job.holder)
                .execute(&self.pool)
                .await
                .map_err(db_err)?;
        Ok(r.rows_affected() > 0)
    }

    async fn reschedule(
        &self,
        job: &ClaimedJob,
        run_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Res<bool> {
        // A deliberate reschedule (delay, retry backoff) is not a failure: reset the attempt count.
        let r = sqlx::query("UPDATE jobs SET status = 'ready', run_at = ?, locked_by = NULL, locked_until = NULL, attempts = 0, updated_at = ? WHERE id = ? AND status = 'running' AND locked_by = ?")
            .bind(run_at)
            .bind(now)
            .bind(job.id.as_uuid())
            .bind(&job.holder)
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(r.rows_affected() > 0)
    }

    async fn fail(
        &self,
        job: &ClaimedJob,
        error: &str,
        retry_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Res<Option<bool>> {
        // A dead job releases its dedupe key so it never blocks a later job for the same work.
        let r = sqlx::query(
            "UPDATE jobs SET status = IF(attempts >= max_attempts, 'dead', 'ready'), dedupe_key = IF(attempts >= max_attempts, NULL, dedupe_key), run_at = ?, last_error = ?, locked_by = NULL, locked_until = NULL, updated_at = ? WHERE id = ? AND status = 'running' AND locked_by = ?",
        )
        .bind(retry_at)
        .bind(error.chars().take(4000).collect::<String>())
        .bind(now)
        .bind(job.id.as_uuid())
        .bind(&job.holder)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        if r.rows_affected() == 0 {
            return Ok(None);
        }
        Ok(Some(job.attempts >= job.max_attempts))
    }

    async fn heartbeat(&self, job: &ClaimedJob, lease_until: DateTime<Utc>) -> Res<bool> {
        let r = sqlx::query("UPDATE jobs SET locked_until = ? WHERE id = ? AND status = 'running' AND locked_by = ?")
            .bind(lease_until)
            .bind(job.id.as_uuid())
            .bind(&job.holder)
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(r.rows_affected() > 0)
    }

    async fn reap(&self, now: DateTime<Utc>) -> Res<Vec<Value>> {
        let mut tx = self.pool.begin().await.map_err(db_err)?;
        // Jobs about to go dead (their worker kept dying): locked so the update below sees them.
        let dying: Vec<Json<Value>> = sqlx::query_scalar("SELECT payload FROM jobs WHERE status = 'running' AND locked_until < ? AND attempts >= max_attempts FOR UPDATE")
            .bind(now)
            .fetch_all(&mut *tx)
            .await
            .map_err(db_err)?;
        // A job whose worker keeps dying (crash, OOM, panic) is a poison job: dead after
        // `max_attempts` claims instead of cycling forever.
        sqlx::query("UPDATE jobs SET status = IF(attempts >= max_attempts, 'dead', 'ready'), dedupe_key = IF(attempts >= max_attempts, NULL, dedupe_key), last_error = IF(attempts >= max_attempts, 'lease expired too many times', last_error), locked_by = NULL, locked_until = NULL, updated_at = ? WHERE status = 'running' AND locked_until < ?")
            .bind(now)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        // Their executions fail in the same transaction, so none is left queued or running
        // without a job (a crash between two separate writes would strand it).
        for payload in &dying {
            let Some(execution) = payload
                .0
                .get("execution_id")
                .and_then(Value::as_str)
                .and_then(|s| s.parse::<ExecutionId>().ok())
            else {
                continue;
            };
            sqlx::query("UPDATE executions SET status = 'failed', error_code = 'internal.poison_job', error_message = 'The execution could not be processed after many attempts', waiting_until = NULL, finished_at = ? WHERE id = ? AND status IN ('queued', 'running', 'waiting')")
                .bind(now)
                .bind(execution.as_uuid())
                .execute(&mut *tx)
                .await
                .map_err(db_err)?;
        }
        tx.commit().await.map_err(db_err)?;
        Ok(dying.into_iter().map(|p| p.0).collect())
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
