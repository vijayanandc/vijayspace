//! Ports: server instances (heartbeats) and queue health, for running several servers on one database.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use super::Res;

/// One running server process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Instance {
    pub id: Uuid,
    /// `api`, `worker` or `all`.
    pub kind: String,
    pub host: String,
    pub version: String,
    pub started_at: DateTime<Utc>,
    pub seen_at: DateTime<Utc>,
}

/// Jobs waiting in one queue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QueueDepth {
    pub queue: String,
    /// Jobs due now and not yet claimed.
    pub ready: u64,
    /// The oldest due job's due time (how far behind the workers are).
    pub oldest_due: Option<DateTime<Utc>>,
}

/// The job queue at a glance.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct QueueStats {
    pub queues: Vec<QueueDepth>,
    pub running: u64,
    pub dead: u64,
}

#[async_trait]
pub trait OpsRepo: Send + Sync {
    /// Records that `instance` is alive at `instance.seen_at` (insert or update).
    async fn instance_heartbeat(&self, instance: &Instance) -> Res<()>;
    /// Instances seen at or after `since`, oldest first.
    async fn live_instances(&self, since: DateTime<Utc>) -> Res<Vec<Instance>>;
    /// Removes an instance (a clean shutdown).
    async fn remove_instance(&self, id: Uuid) -> Res<()>;
    /// Queue depth and lag as of `now`.
    async fn queue_stats(&self, now: DateTime<Utc>) -> Res<QueueStats>;
}
