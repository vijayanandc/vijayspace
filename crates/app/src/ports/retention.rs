//! Ports: run history retention (doc 05 §4): day partitions dropped at the 30-day cap, per-org shorter
//! retention and platform housekeeping in small batches.

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
    /// Invitations that expired (or were revoked) more than a day ago.
    pub invitations: u64,
    /// Trigger dedupe keys older than the history cap.
    pub dedupe_keys: u64,
    /// Test-run snapshots older than the history cap that no run uses.
    pub test_snapshots: u64,
}

/// What one partition maintenance did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
pub struct PartitionReport {
    /// Day partitions created ahead (all tables).
    pub created: u32,
    /// Day partitions dropped past the cap (all tables); their space goes back to the OS.
    pub dropped: u32,
    /// Runs still unfinished in a partition about to be dropped, marked failed (`execution.expired`).
    pub expired_runs: u64,
    /// Rows in the catch-all partition (maintenance lapsed): they cannot be dropped until it runs again.
    pub rows_in_catch_all: u64,
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
    /// `history_before`: trigger dedupe keys and unused test snapshots older than this go too.
    async fn housekeeping(
        &self,
        jobs_before: DateTime<Utc>,
        now: DateTime<Utc>,
        usage_before: NaiveDate,
        history_before: DateTime<Utc>,
        limit: u32,
    ) -> Res<Housekeeping>;
    /// Keeps the run history partitions: day partitions through `today + ahead_days`, and drops those older
    /// than `today - keep_days` (after failing runs still unfinished in them).
    async fn maintain_partitions(
        &self,
        today: NaiveDate,
        keep_days: u32,
        ahead_days: u32,
        now: DateTime<Utc>,
    ) -> Res<PartitionReport>;
}
