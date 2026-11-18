//! Ports: deleting old run history in small batches (doc 05 §4).

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use weft_domain::ids::OrgId;

use super::Res;

/// Rows removed by one batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Purged {
    pub executions: u64,
    pub step_runs: u64,
    pub trigger_events: u64,
    pub test_snapshots: u64,
}

/// Platform-wide housekeeping rows removed by one call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Housekeeping {
    pub dead_jobs: u64,
    pub oauth_states: u64,
    pub sync_leases: u64,
    pub usage_days: u64,
}

#[async_trait]
pub trait RetentionRepo: Send + Sync {
    /// Deletes up to `limit` *finished* executions (succeeded, failed, cancelled) of `org` that
    /// finished before `before`, with their step runs; then up to `limit` trigger events received
    /// before `before` that no execution points to, and test snapshots no execution uses.
    /// Queued, running and waiting executions are never touched.
    async fn purge_history(&self, org: OrgId, before: DateTime<Utc>, limit: u32) -> Res<Purged>;
    /// Removes dead jobs last updated before `jobs_before`, expired OAuth states and sync leases
    /// (as of `now`) and usage counters older than `usage_before`, at most `limit` rows each.
    async fn housekeeping(
        &self,
        jobs_before: DateTime<Utc>,
        now: DateTime<Utc>,
        usage_before: NaiveDate,
        limit: u32,
    ) -> Res<Housekeeping>;
}
