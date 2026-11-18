//! Ports: server instances (heartbeats), queue health and the facts staff alerts are made of.

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

/// An org and how many of its runs failed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OrgFailures {
    pub org_id: Uuid,
    pub org_name: String,
    pub failed: u64,
}

/// A connection that needs signing in again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StaleConnection {
    pub org_id: Uuid,
    pub org_name: String,
    pub connection_id: Uuid,
    pub name: String,
    pub connector: String,
}

/// What staff alerts are checked against.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct AlertFacts {
    /// Jobs that became dead since the window started.
    pub new_dead_jobs: u64,
    /// Orgs with failed runs that finished in the window.
    pub failed_runs: Vec<OrgFailures>,
    /// Connections whose status is `needs_reauth` (at most 50).
    pub needs_reauth: Vec<StaleConnection>,
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
    /// Dead jobs, failed runs and stale connections since `since` (runs: only today's and yesterday's partitions).
    async fn alert_facts(&self, since: DateTime<Utc>) -> Res<AlertFacts>;
}
