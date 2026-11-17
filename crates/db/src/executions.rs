//! `ExecutionRepo` for MySQL: executions, step runs, trigger events and usage counters.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::{Map, Value, json};
use sqlx::{FromRow, MySql, QueryBuilder, Transaction, types::Json};
use weft_app::ports::{
    Checkpoint, ExecutionFilter, ExecutionRepo, Finish, NewExecution, NewJob, Res, StepCompletion,
};
use weft_domain::{
    ids::{ExecutionId, FlowId, OrgId},
    model::{Execution, StepRun},
};

use crate::{
    MySqlStore,
    codec::{db_err, enum_str},
    jobs::insert_job,
    rows::{ExecutionRow, StepRunRow},
    usage::bump_usage,
};

const EXE_COLUMNS: &str = "e.id, e.org_id, e.flow_id, e.flow_version_id, e.trigger_event_id, e.mode, e.rerun_of, e.status, e.trigger_data, e.run_state, \
    e.waiting_until, e.task_count, e.error_code, e.error_message, e.failed_step_key, e.possible_duplicate, e.cancel_requested, e.queued_at, e.started_at, e.finished_at";

#[derive(Debug, FromRow)]
struct ExecutionListRow {
    #[sqlx(flatten)]
    exe: ExecutionRow,
    flow_name: String,
}

pub(crate) async fn insert_execution(
    tx: &mut Transaction<'_, MySql>,
    e: &Execution,
    now: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO executions (id, org_id, flow_id, flow_version_id, trigger_event_id, mode, rerun_of, status, trigger_data, run_state, queued_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(e.id.as_uuid())
    .bind(e.org_id.as_uuid())
    .bind(e.flow_id.as_uuid())
    .bind(e.flow_version_id.as_uuid())
    .bind(e.trigger_event_id.map(|t| t.as_uuid()))
    .bind(enum_str(&e.mode))
    .bind(e.rerun_of.map(|r| r.as_uuid()))
    .bind(enum_str(&e.status))
    .bind(Json(&e.trigger_data))
    .bind(Json(&e.run_state))
    .bind(e.queued_at)
    .bind(now)
    .execute(&mut **tx)
    .await
    .map(|_| ())
}

async fn write_checkpoint(
    tx: &mut Transaction<'_, MySql>,
    c: &Checkpoint,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE executions SET status = ?, run_state = ?, waiting_until = ?, updated_at = ? WHERE id = ?")
        .bind(enum_str(&c.status))
        .bind(Json(&c.run_state))
        .bind(c.waiting_until)
        .bind(c.now)
        .bind(c.execution_id.as_uuid())
        .execute(&mut **tx)
        .await
        .map(|_| ())
}

#[async_trait]
impl ExecutionRepo for MySqlStore {
    async fn create_executions(
        &self,
        org: OrgId,
        _flow: FlowId,
        items: &[NewExecution],
        now: DateTime<Utc>,
    ) -> Res<Vec<ExecutionId>> {
        // Concurrent deliveries for one record can deadlock (InnoDB picks a victim): the whole
        // transaction is safe to repeat, so retry a few times before giving up.
        let mut attempt = 0;
        loop {
            match self.create_executions_once(org, items, now).await {
                Err(e) if e.code == weft_domain::error_codes::DB_DEADLOCK && attempt < 3 => {
                    attempt += 1;
                }
                other => return other,
            }
        }
    }

    async fn get_execution(&self, org: OrgId, id: ExecutionId) -> Res<Option<Execution>> {
        let sql = format!("SELECT {EXE_COLUMNS} FROM executions e WHERE e.org_id = ? AND e.id = ?");
        let row: Option<ExecutionRow> = sqlx::query_as(&sql)
            .bind(org.as_uuid())
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(db_err)?;
        row.map(Execution::try_from).transpose()
    }

    async fn get_execution_system(&self, id: ExecutionId) -> Res<Option<Execution>> {
        let sql = format!("SELECT {EXE_COLUMNS} FROM executions e WHERE e.id = ?");
        let row: Option<ExecutionRow> = sqlx::query_as(&sql)
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(db_err)?;
        row.map(Execution::try_from).transpose()
    }

    async fn list_executions(
        &self,
        org: OrgId,
        filter: &ExecutionFilter,
    ) -> Res<Vec<(Execution, String)>> {
        let mut qb = QueryBuilder::new(format!(
            "SELECT {EXE_COLUMNS}, f.name AS flow_name FROM executions e JOIN flows f ON f.id = e.flow_id WHERE e.org_id = "
        ));
        qb.push_bind(org.as_uuid());
        if let Some(flow) = filter.flow_id {
            qb.push(" AND e.flow_id = ").push_bind(flow.as_uuid());
        }
        if let Some(status) = filter.status {
            qb.push(" AND e.status = ").push_bind(enum_str(&status));
        }
        if let Some((at, id)) = filter.before {
            qb.push(" AND (e.queued_at < ")
                .push_bind(at)
                .push(" OR (e.queued_at = ")
                .push_bind(at)
                .push(" AND e.id < ")
                .push_bind(id.as_uuid())
                .push("))");
        }
        qb.push(" ORDER BY e.queued_at DESC, e.id DESC LIMIT ")
            .push_bind(filter.limit);
        let rows: Vec<ExecutionListRow> = qb
            .build_query_as()
            .fetch_all(&self.pool)
            .await
            .map_err(db_err)?;
        rows.into_iter()
            .map(|r| Ok((Execution::try_from(r.exe)?, r.flow_name)))
            .collect()
    }

    async fn list_step_runs(&self, execution: ExecutionId) -> Res<Vec<StepRun>> {
        let rows: Vec<StepRunRow> = sqlx::query_as(
            "SELECT id, execution_id, seq, step_key, kind, attempt, status, input, output, error, warnings, started_at, finished_at FROM step_runs WHERE execution_id = ? ORDER BY seq",
        )
        .bind(execution.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;
        rows.into_iter().map(StepRun::try_from).collect()
    }

    async fn load_step_outputs(&self, execution: ExecutionId) -> Res<Map<String, Value>> {
        #[derive(FromRow)]
        struct OutRow {
            step_key: String,
            status: String,
            output: Option<Json<Value>>,
            error: Option<Json<Value>>,
        }
        let rows: Vec<OutRow> = sqlx::query_as("SELECT step_key, status, output, error FROM step_runs WHERE execution_id = ? AND kind = 'action' AND status IN ('succeeded', 'failed') ORDER BY seq")
            .bind(execution.as_uuid())
            .fetch_all(&self.pool)
            .await
            .map_err(db_err)?;
        let mut outputs = Map::new();
        for r in rows {
            let value = if r.status == "succeeded" {
                r.output.map_or(Value::Null, |o| o.0)
            } else {
                let err = r.error.map_or(Value::Null, |e| e.0);
                json!({"error": {"code": err.get("code"), "message": err.get("message")}})
            };
            outputs.insert(r.step_key, value);
        }
        Ok(outputs)
    }

    async fn insert_step_run(&self, org: OrgId, run: &StepRun) -> Res<()> {
        sqlx::query(
            "INSERT INTO step_runs (id, org_id, execution_id, seq, step_key, kind, attempt, status, input, output, error, warnings, started_at, finished_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(run.id.as_uuid())
        .bind(org.as_uuid())
        .bind(run.execution_id.as_uuid())
        .bind(run.seq)
        .bind(&run.step_key)
        .bind(&run.kind)
        .bind(run.attempt)
        .bind(enum_str(&run.status))
        .bind(run.input.as_ref().map(Json))
        .bind(run.output.as_ref().map(Json))
        .bind(run.error.as_ref().map(Json))
        .bind((!run.warnings.is_empty()).then_some(Json(&run.warnings)))
        .bind(run.started_at)
        .bind(run.finished_at)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn complete_step(&self, c: &StepCompletion) -> Res<()> {
        let mut tx = self.pool.begin().await.map_err(db_err)?;
        sqlx::query(
            "UPDATE step_runs SET status = ?, output = ?, error = ?, finished_at = ? WHERE id = ?",
        )
        .bind(enum_str(&c.status))
        .bind(c.output.as_ref().map(Json))
        .bind(c.error.as_ref().map(Json))
        .bind(c.finished_at)
        .bind(c.step_run_id.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;
        write_checkpoint(&mut tx, &c.execution)
            .await
            .map_err(db_err)?;
        if c.count_task {
            sqlx::query("UPDATE executions SET task_count = task_count + 1 WHERE id = ?")
                .bind(c.execution.execution_id.as_uuid())
                .execute(&mut *tx)
                .await
                .map_err(db_err)?;
            bump_usage(&mut tx, c.execution.org_id, c.finished_at, 1, 0)
                .await
                .map_err(db_err)?;
        }
        tx.commit().await.map_err(db_err)
    }

    async fn checkpoint(&self, c: &Checkpoint) -> Res<()> {
        let mut tx = self.pool.begin().await.map_err(db_err)?;
        write_checkpoint(&mut tx, c).await.map_err(db_err)?;
        tx.commit().await.map_err(db_err)
    }

    async fn mark_started(&self, id: ExecutionId, now: DateTime<Utc>) -> Res<()> {
        sqlx::query("UPDATE executions SET status = 'running', started_at = COALESCE(started_at, ?), waiting_until = NULL, updated_at = ? WHERE id = ?")
            .bind(now)
            .bind(now)
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(())
    }

    async fn mark_possible_duplicate(&self, id: ExecutionId) -> Res<()> {
        sqlx::query("UPDATE executions SET possible_duplicate = TRUE WHERE id = ?")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(())
    }

    async fn mark_unknown_outcomes(&self, id: ExecutionId, now: DateTime<Utc>) -> Res<u64> {
        let r = sqlx::query("UPDATE step_runs SET status = 'unknown_outcome', finished_at = ? WHERE execution_id = ? AND kind = 'action' AND status = 'running'")
            .bind(now)
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(r.rows_affected())
    }

    async fn finish_execution(&self, f: &Finish) -> Res<()> {
        sqlx::query(
            "UPDATE executions SET status = ?, run_state = ?, error_code = ?, error_message = ?, failed_step_key = ?, finished_at = ?, waiting_until = NULL, updated_at = ? WHERE id = ?",
        )
        .bind(enum_str(&f.status))
        .bind(Json(&f.run_state))
        .bind(&f.error_code)
        .bind(&f.error_message)
        .bind(&f.failed_step_key)
        .bind(f.now)
        .bind(f.now)
        .bind(f.execution_id.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn request_cancel(&self, org: OrgId, id: ExecutionId, now: DateTime<Utc>) -> Res<()> {
        sqlx::query("UPDATE executions SET cancel_requested = TRUE, updated_at = ? WHERE org_id = ? AND id = ?").bind(now).bind(org.as_uuid()).bind(id.as_uuid()).execute(&self.pool).await.map_err(db_err)?;
        Ok(())
    }

    async fn requeue_execution(
        &self,
        id: ExecutionId,
        run_state: &Value,
        job: &NewJob,
        now: DateTime<Utc>,
    ) -> Res<()> {
        let mut tx = self.pool.begin().await.map_err(db_err)?;
        sqlx::query("UPDATE executions SET status = 'queued', run_state = ?, error_code = NULL, error_message = NULL, failed_step_key = NULL, finished_at = NULL, updated_at = ? WHERE id = ?")
            .bind(Json(run_state))
            .bind(now)
            .bind(id.as_uuid())
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        sqlx::query("DELETE FROM jobs WHERE dedupe_key = ? AND status <> 'running'")
            .bind(format!("exe:{id}"))
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        insert_job(&mut *tx, job, now).await.map_err(db_err)?;
        tx.commit().await.map_err(db_err)
    }

    async fn next_step_seq(&self, execution: ExecutionId) -> Res<u32> {
        let max: Option<u32> =
            sqlx::query_scalar("SELECT MAX(seq) FROM step_runs WHERE execution_id = ?")
                .bind(execution.as_uuid())
                .fetch_one(&self.pool)
                .await
                .map_err(db_err)?;
        Ok(max.map_or(1, |m| m + 1))
    }

    async fn usage(&self, org: OrgId, period: &str) -> Res<(u32, u32)> {
        let row: Option<(u32, u32)> = sqlx::query_as(
            "SELECT tasks_used, executions FROM usage_counters WHERE org_id = ? AND period = ?",
        )
        .bind(org.as_uuid())
        .bind(period)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(row.unwrap_or((0, 0)))
    }
}
