//! Ports: executions, step runs and their checkpoints.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::{Map, Value};
use weft_domain::{
    ids::{ExecutionId, FlowId, OrgId, RegistrationId, StepRunId, TriggerEventId},
    model::{Execution, ExecutionStatus, StepRun, StepStatus},
};

use super::{NewJob, Res};

/// A trigger event plus the execution it starts and the job that runs it (inserted atomically).
#[derive(Debug, Clone, PartialEq)]
pub struct NewExecution {
    pub event: Option<NewTriggerEvent>,
    pub execution: Execution,
    pub job: NewJob,
    /// Debounced app events: the item's own dedupe key (record + version), recorded as an extra
    /// event so a later delivery of the same version (e.g. the reconciliation poll) is skipped.
    /// `event` then carries the per-burst key.
    pub seen: Option<NewTriggerEvent>,
    /// When `event`'s key already exists, replace the trigger data of that burst's execution if
    /// it has not started yet (the newest state wins). If it has started, start a new execution.
    pub coalesce: bool,
}

/// A received trigger payload.
#[derive(Debug, Clone, PartialEq)]
pub struct NewTriggerEvent {
    pub id: TriggerEventId,
    pub registration_id: RegistrationId,
    pub dedupe_key: Option<String>,
    pub payload: Value,
}

/// Filter for execution lists (cursor = `(queued_at, id)` of the last row seen).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExecutionFilter {
    pub flow_id: Option<FlowId>,
    pub status: Option<ExecutionStatus>,
    pub limit: u32,
    pub before: Option<(DateTime<Utc>, ExecutionId)>,
}

/// Completing one action attempt: step run result + engine state + usage, in one transaction.
#[derive(Debug, Clone, PartialEq)]
pub struct StepCompletion {
    pub step_run_id: StepRunId,
    pub status: StepStatus,
    pub output: Option<Value>,
    pub error: Option<Value>,
    pub finished_at: DateTime<Utc>,
    pub execution: Checkpoint,
    pub count_task: bool,
}

/// New execution-level state.
#[derive(Debug, Clone, PartialEq)]
pub struct Checkpoint {
    pub execution_id: ExecutionId,
    pub org_id: OrgId,
    pub status: ExecutionStatus,
    pub run_state: Value,
    pub waiting_until: Option<DateTime<Utc>>,
    pub now: DateTime<Utc>,
}

/// Final outcome of an execution.
#[derive(Debug, Clone, PartialEq)]
pub struct Finish {
    pub execution_id: ExecutionId,
    pub status: ExecutionStatus,
    pub run_state: Value,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub failed_step_key: Option<String>,
    pub now: DateTime<Utc>,
}

#[async_trait]
pub trait ExecutionRepo: Send + Sync {
    /// Inserts events + executions + jobs atomically. Events whose dedupe key already exists are
    /// skipped together with their execution. Returns the IDs actually created.
    async fn create_executions(
        &self,
        org: OrgId,
        flow: FlowId,
        items: &[NewExecution],
        now: DateTime<Utc>,
    ) -> Res<Vec<ExecutionId>>;
    async fn get_execution(&self, org: OrgId, id: ExecutionId) -> Res<Option<Execution>>;
    async fn get_execution_system(&self, id: ExecutionId) -> Res<Option<Execution>>;
    /// Whether a cancel was asked for and the run has not finished (one small read, no payloads).
    async fn cancel_requested(&self, id: ExecutionId) -> Res<bool>;
    async fn list_executions(
        &self,
        org: OrgId,
        filter: &ExecutionFilter,
    ) -> Res<Vec<(Execution, String)>>;
    async fn list_step_runs(&self, execution: ExecutionId) -> Res<Vec<StepRun>>;
    /// Latest output per step key (succeeded, or `{"error":…}` for continued failures).
    async fn load_step_outputs(&self, execution: ExecutionId) -> Res<Map<String, Value>>;
    async fn insert_step_run(&self, org: OrgId, run: &StepRun) -> Res<()>;
    /// Parks an execution whose step was held back by a limit, in one transaction: the step run
    /// (which turned out not to be an attempt) is removed and the execution checkpointed.
    async fn park_held_back_step(&self, step_run: StepRunId, checkpoint: &Checkpoint) -> Res<()>;
    async fn complete_step(&self, completion: &StepCompletion) -> Res<()>;
    async fn checkpoint(&self, checkpoint: &Checkpoint) -> Res<()>;
    async fn mark_started(&self, id: ExecutionId, now: DateTime<Utc>) -> Res<()>;
    async fn mark_possible_duplicate(&self, id: ExecutionId) -> Res<()>;
    /// Marks steps left `running` by a crash as `unknown_outcome`; returns how many.
    async fn mark_unknown_outcomes(&self, id: ExecutionId, now: DateTime<Utc>) -> Res<u64>;
    async fn finish_execution(&self, finish: &Finish) -> Res<()>;
    async fn request_cancel(&self, org: OrgId, id: ExecutionId, now: DateTime<Utc>) -> Res<()>;
    /// Requeues a failed execution (retry from the failed step) and enqueues its job.
    async fn requeue_execution(
        &self,
        id: ExecutionId,
        run_state: &Value,
        job: &NewJob,
        now: DateTime<Utc>,
    ) -> Res<()>;
    async fn next_step_seq(&self, execution: ExecutionId) -> Res<u32>;
    async fn usage(&self, org: OrgId, period: &str) -> Res<(u32, u32)>;
}
