//! `OpsRepo` for MySQL: server instance heartbeats and job queue depth.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;
use weft_app::ports::{Instance, OpsRepo, QueueDepth, QueueStats, Res};

use crate::{MySqlStore, codec::db_err};

type InstanceRow = (Uuid, String, String, String, DateTime<Utc>, DateTime<Utc>);

#[async_trait]
impl OpsRepo for MySqlStore {
    async fn instance_heartbeat(&self, i: &Instance) -> Res<()> {
        sqlx::query(
            "INSERT INTO instances (id, kind, host, version, started_at, seen_at) VALUES (?, ?, ?, ?, ?, ?) AS new \
             ON DUPLICATE KEY UPDATE seen_at = new.seen_at",
        )
        .bind(i.id)
        .bind(&i.kind)
        .bind(&i.host)
        .bind(&i.version)
        .bind(i.started_at)
        .bind(i.seen_at)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn live_instances(&self, since: DateTime<Utc>) -> Res<Vec<Instance>> {
        let rows: Vec<InstanceRow> = sqlx::query_as(
            "SELECT id, kind, host, version, started_at, seen_at FROM instances WHERE seen_at >= ? ORDER BY started_at",
        )
        .bind(since)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(rows
            .into_iter()
            .map(|(id, kind, host, version, started_at, seen_at)| Instance {
                id,
                kind,
                host,
                version,
                started_at,
                seen_at,
            })
            .collect())
    }

    async fn remove_instance(&self, id: Uuid) -> Res<()> {
        sqlx::query("DELETE FROM instances WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(())
    }

    async fn queue_stats(&self, now: DateTime<Utc>) -> Res<QueueStats> {
        let ready: Vec<(String, i64, Option<DateTime<Utc>>)> = sqlx::query_as(
            "SELECT queue, COUNT(*), MIN(run_at) FROM jobs WHERE status = 'ready' AND run_at <= ? GROUP BY queue ORDER BY queue",
        )
        .bind(now)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;
        let counts: Vec<(String, i64)> = sqlx::query_as(
            "SELECT status, COUNT(*) FROM jobs WHERE status IN ('running', 'dead') GROUP BY status",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;
        let count = |status: &str| {
            counts
                .iter()
                .find(|(s, _)| s == status)
                .map_or(0, |(_, n)| u64::try_from(*n).unwrap_or(0))
        };
        Ok(QueueStats {
            queues: ready
                .into_iter()
                .map(|(queue, n, oldest_due)| QueueDepth {
                    queue,
                    ready: u64::try_from(n).unwrap_or(0),
                    oldest_due,
                })
                .collect(),
            running: count("running"),
            dead: count("dead"),
        })
    }
}
