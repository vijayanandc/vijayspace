//! `LogRepo` for MySQL: a run's detailed log lines (`execution_logs`, in the run's day partition).

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::{QueryBuilder, types::Json};
use uuid::Uuid;
use weft_app::ports::{LogKind, LogLevel, LogRecord, LogRepo, Res};
use weft_domain::ids::{ExecutionId, OrgId};

use crate::{MySqlStore, codec::db_err};

/// The longest stored message (the column is `VARCHAR(500)`).
const MAX_MESSAGE_CHARS: usize = 500;

type LogRow = (
    Uuid,
    Option<String>,
    Option<u16>,
    String,
    String,
    String,
    Option<Json<serde_json::Value>>,
    DateTime<Utc>,
);

#[async_trait]
impl LogRepo for MySqlStore {
    async fn append_logs(
        &self,
        org: OrgId,
        execution: ExecutionId,
        records: &[LogRecord],
    ) -> Res<()> {
        if records.is_empty() {
            return Ok(());
        }
        let day: Option<NaiveDate> =
            sqlx::query_scalar("SELECT day FROM executions WHERE org_id = ? AND id = ?")
                .bind(org.as_uuid())
                .bind(execution.as_uuid())
                .fetch_optional(&self.pool)
                .await
                .map_err(db_err)?;
        let Some(day) = day else {
            return Ok(());
        };
        let mut qb = QueryBuilder::new(
            "INSERT INTO execution_logs (id, org_id, execution_id, day, step_key, attempt, level, kind, message, details, at) ",
        );
        qb.push_values(records, |mut row, r| {
            row.push_bind(r.id)
                .push_bind(org.as_uuid())
                .push_bind(execution.as_uuid())
                .push_bind(day)
                .push_bind(r.step_key.clone())
                .push_bind(r.attempt)
                .push_bind(r.level.as_str())
                .push_bind(r.kind.as_str())
                .push_bind(
                    r.message
                        .chars()
                        .take(MAX_MESSAGE_CHARS)
                        .collect::<String>(),
                )
                .push_bind(r.details.clone().map(Json))
                .push_bind(r.at);
        });
        qb.build().execute(&self.pool).await.map_err(db_err)?;
        Ok(())
    }

    async fn list_logs(
        &self,
        org: OrgId,
        execution: ExecutionId,
        limit: u32,
    ) -> Res<Vec<LogRecord>> {
        let rows: Vec<LogRow> = sqlx::query_as(
            "SELECT id, step_key, attempt, level, kind, message, details, at FROM execution_logs \
             WHERE org_id = ? AND execution_id = ? ORDER BY at DESC, id DESC LIMIT ?",
        )
        .bind(org.as_uuid())
        .bind(execution.as_uuid())
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;
        // The newest lines (where a long run failed) are kept when there are more than `limit`; oldest first.
        Ok(rows
            .into_iter()
            .rev()
            .map(
                |(id, step_key, attempt, level, kind, message, details, at)| LogRecord {
                    id,
                    execution_id: execution,
                    step_key,
                    attempt,
                    level: LogLevel::parse(&level),
                    kind: LogKind::parse(&kind),
                    message,
                    details: details.map(|j| j.0),
                    at,
                },
            )
            .collect())
    }
}
