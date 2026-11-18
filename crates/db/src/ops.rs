//! `OpsRepo` for MySQL: server instance heartbeats and job queue depth.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;
use weft_app::ports::{
    AlertFacts, Instance, OpsRepo, OrgFailures, QueueDepth, QueueStats, Res, StaleConnection,
};

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

    async fn alert_facts(&self, since: DateTime<Utc>) -> Res<AlertFacts> {
        let new_dead_jobs: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM jobs WHERE status = 'dead' AND updated_at >= ?",
        )
        .bind(since)
        .fetch_one(&self.pool)
        .await
        .map_err(db_err)?;
        // `day` prunes to the window's partitions (a run's day is when it was queued: at most a day before).
        let failed: Vec<(Uuid, String, i64)> = sqlx::query_as(
            // Per org, through (org_id, status, queued_at): only failed runs are read, whatever day they were
            // queued (a delayed or retried run can fail weeks after it was queued).
            "SELECT o.id, o.name, COUNT(*) FROM orgs o STRAIGHT_JOIN executions e FORCE INDEX (ix_executions_org_status) \
             ON e.org_id = o.id AND e.status = 'failed' \
             WHERE e.finished_at >= ? GROUP BY o.id, o.name ORDER BY COUNT(*) DESC LIMIT 50",
        )
        .bind(since)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;
        let stale: Vec<(Uuid, String, Uuid, String, String)> = sqlx::query_as(
            "SELECT c.org_id, o.name, c.id, c.name, c.connector_key FROM connections c JOIN orgs o ON o.id = c.org_id \
             WHERE c.status = 'needs_reauth' AND c.deleted_at IS NULL ORDER BY c.updated_at LIMIT 50",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(AlertFacts {
            new_dead_jobs: u64::try_from(new_dead_jobs).unwrap_or(0),
            failed_runs: failed
                .into_iter()
                .map(|(org_id, org_name, n)| OrgFailures {
                    org_id,
                    org_name,
                    failed: u64::try_from(n).unwrap_or(0),
                })
                .collect(),
            needs_reauth: stale
                .into_iter()
                .map(
                    |(org_id, org_name, connection_id, name, connector)| StaleConnection {
                        org_id,
                        org_name,
                        connection_id,
                        name,
                        connector,
                    },
                )
                .collect(),
        })
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
