//! Ports: The job queue.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use weft_domain::ids::{ExecutionId, JobId, OrgId};

use super::Res;

/// A job to enqueue.
#[derive(Debug, Clone, PartialEq)]
pub struct NewJob {
    pub id: JobId,
    pub queue: String,
    pub kind: String,
    pub org_id: Option<OrgId>,
    pub payload: Value,
    pub run_at: DateTime<Utc>,
    pub dedupe_key: Option<String>,
}

/// A claimed job.
#[derive(Debug, Clone, PartialEq)]
pub struct ClaimedJob {
    pub id: JobId,
    pub queue: String,
    pub kind: String,
    pub payload: Value,
    pub attempts: u32,
    pub max_attempts: u32,
    /// The worker that holds the lease; settling calls only affect a job it still holds.
    pub holder: String,
}

#[async_trait]
pub trait JobQueue: Send + Sync {
    /// Enqueues unless a job with the same dedupe key exists (returns false then).
    async fn enqueue(&self, job: &NewJob, now: DateTime<Utc>) -> Res<bool>;
    async fn claim(
        &self,
        queues: &[&str],
        worker: &str,
        limit: u32,
        now: DateTime<Utc>,
        lease_until: DateTime<Utc>,
    ) -> Res<Vec<ClaimedJob>>;
    /// Settling calls (`complete`, `reschedule`, `fail`, `heartbeat`) only touch a job whose lease
    /// `job.holder` still holds; they return `false` when the lease was lost (reaped and reclaimed).
    async fn complete(&self, job: &ClaimedJob) -> Res<bool>;
    async fn reschedule(
        &self,
        job: &ClaimedJob,
        run_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Res<bool>;
    /// Records a failure; the job is retried at `retry_at` or marked dead after `max_attempts`.
    /// Returns `Some(true)` when it went dead, `None` when the lease was lost.
    async fn fail(
        &self,
        job: &ClaimedJob,
        error: &str,
        retry_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Res<Option<bool>>;
    async fn heartbeat(&self, job: &ClaimedJob, lease_until: DateTime<Utc>) -> Res<bool>;
    /// Returns expired leases to `ready`, or marks them dead after `max_attempts` (poison jobs).
    /// Executions of jobs that went dead are failed in the same transaction; returns the dead
    /// jobs' payloads.
    async fn reap(&self, now: DateTime<Utc>) -> Res<Vec<Value>>;
    async fn delete_jobs_for_execution(&self, execution: ExecutionId) -> Res<()>;
}
