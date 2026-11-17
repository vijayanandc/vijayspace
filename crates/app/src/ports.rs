//! Ports: the interfaces use cases depend on. `weft-db` and `weft-infra` implement them.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::{Map, Value};
use uuid::Uuid;
use weft_domain::{
    flow::FlowDefinition,
    ids::{
        ConnectionId, ExecutionId, FlowId, FlowVersionId, JobId, LinkId, OrgId, RegistrationId,
        StepRunId, SubscriptionId, TriggerEventId,
    },
    model::{
        Connection, ConnectionStatus, Execution, ExecutionStatus, Flow, FlowVersion, Org, StepRun,
        StepStatus, TriggerRegistration,
    },
};

use crate::error::AppError;

/// Result alias for port methods.
pub type Res<T> = Result<T, AppError>;

/// Wall clock (truncated to microseconds). Tests use a manual clock.
pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

/// `UUIDv7` generator. Tests use a deterministic sequence.
pub trait IdGen: Send + Sync {
    fn uuid(&self) -> Uuid;
}

/// Authenticated encryption for connection secrets (AAD binds a ciphertext to its row).
pub trait SecretBox: Send + Sync {
    /// Returns `(ciphertext, key_id)`.
    fn seal(&self, plaintext: &[u8], aad: &[u8]) -> Res<(Vec<u8>, String)>;
    fn open(&self, ciphertext: &[u8], key_id: &str, aad: &[u8]) -> Res<Vec<u8>>;
}

/// A flow row for list screens.
#[derive(Debug, Clone)]
pub struct FlowListRow {
    pub flow: Flow,
    pub last_execution: Option<(ExecutionId, ExecutionStatus, DateTime<Utc>)>,
}

#[async_trait]
pub trait OrgRepo: Send + Sync {
    /// Returns the default org, creating it on first use.
    async fn ensure_default_org(&self, id: OrgId, now: DateTime<Utc>) -> Res<Org>;
    async fn get_org(&self, id: OrgId) -> Res<Option<Org>>;
}

#[async_trait]
pub trait FlowRepo: Send + Sync {
    /// Inserts a flow together with its trigger registration.
    async fn insert_flow(&self, flow: &Flow, registration: &TriggerRegistration) -> Res<()>;
    async fn get_flow(&self, org: OrgId, id: FlowId) -> Res<Option<Flow>>;
    /// Workers and ingress: no tenant scope.
    async fn get_flow_system(&self, id: FlowId) -> Res<Option<Flow>>;
    async fn list_flows(&self, org: OrgId) -> Res<Vec<FlowListRow>>;
    async fn update_flow_meta(
        &self,
        org: OrgId,
        id: FlowId,
        name: &str,
        description: Option<&str>,
        now: DateTime<Utc>,
    ) -> Res<()>;
    /// Saves the draft iff `expected_revision` matches; returns the new revision.
    async fn save_draft(
        &self,
        org: OrgId,
        id: FlowId,
        def: &FlowDefinition,
        expected_revision: u32,
        now: DateTime<Utc>,
    ) -> Res<u32>;
    async fn soft_delete_flow(&self, org: OrgId, id: FlowId, now: DateTime<Utc>) -> Res<()>;
    /// Inserts a published version (number = previous + 1) and points the flow at it.
    async fn publish_version(&self, version: &FlowVersion, now: DateTime<Utc>) -> Res<u32>;
    async fn insert_test_snapshot(&self, version: &FlowVersion) -> Res<()>;
    async fn get_version(&self, id: FlowVersionId) -> Res<Option<FlowVersion>>;
    async fn list_versions(&self, org: OrgId, flow: FlowId) -> Res<Vec<FlowVersion>>;
    async fn set_enabled(
        &self,
        org: OrgId,
        id: FlowId,
        enabled: bool,
        now: DateTime<Utc>,
    ) -> Res<()>;
}

/// A provider-side webhook subscription Weft registered.
#[derive(Debug, Clone, PartialEq)]
pub struct Subscription {
    pub id: SubscriptionId,
    pub org_id: OrgId,
    pub registration_id: RegistrationId,
    pub connection_id: ConnectionId,
    pub connector: String,
    pub external_id: String,
    pub data: Value,
    pub status: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub renew_after: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

#[async_trait]
pub trait TriggerRepo: Send + Sync {
    async fn get_registration_by_flow(&self, flow: FlowId) -> Res<Option<TriggerRegistration>>;
    async fn get_registration_by_token(&self, token: &str) -> Res<Option<TriggerRegistration>>;
    async fn get_registration(&self, id: RegistrationId) -> Res<Option<TriggerRegistration>>;
    /// Updates kind, activity, next fire time and state.
    async fn update_registration(&self, reg: &TriggerRegistration, now: DateTime<Utc>) -> Res<()>;
    async fn set_listening(
        &self,
        id: RegistrationId,
        until: Option<DateTime<Utc>>,
        now: DateTime<Utc>,
    ) -> Res<()>;
    async fn set_sample(&self, id: RegistrationId, sample: &Value, now: DateTime<Utc>) -> Res<()>;
    async fn set_state(&self, id: RegistrationId, state: &Value, now: DateTime<Utc>) -> Res<()>;
    /// Claims due registrations of `kinds` (SKIP LOCKED). Each claimed row's `next_fire_at` is moved
    /// to `lease_until` so a crash cannot lose it; the returned rows carry the original due time.
    async fn take_due(
        &self,
        kinds: &[&str],
        now: DateTime<Utc>,
        lease_until: DateTime<Utc>,
        limit: u32,
    ) -> Res<Vec<TriggerRegistration>>;
    async fn set_next_fire(
        &self,
        id: RegistrationId,
        next: Option<DateTime<Utc>>,
        now: DateTime<Utc>,
    ) -> Res<()>;
    async fn upsert_subscription(&self, sub: &Subscription, now: DateTime<Utc>) -> Res<()>;
    async fn get_subscription(&self, registration: RegistrationId) -> Res<Option<Subscription>>;
    async fn delete_subscription(&self, registration: RegistrationId) -> Res<()>;
    async fn due_subscriptions(&self, now: DateTime<Utc>, limit: u32) -> Res<Vec<Subscription>>;
}

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
    async fn complete(&self, id: JobId) -> Res<()>;
    async fn reschedule(&self, id: JobId, run_at: DateTime<Utc>, now: DateTime<Utc>) -> Res<()>;
    /// Records a failure; the job is retried at `retry_at` or marked dead after `max_attempts`.
    async fn fail(
        &self,
        id: JobId,
        error: &str,
        retry_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Res<bool>;
    async fn heartbeat(&self, id: JobId, lease_until: DateTime<Utc>) -> Res<()>;
    /// Returns expired leases to `ready`.
    async fn reap(&self, now: DateTime<Utc>) -> Res<u64>;
    async fn delete_jobs_for_execution(&self, execution: ExecutionId) -> Res<()>;
}

/// A trigger event plus the execution it starts and the job that runs it (inserted atomically).
#[derive(Debug, Clone, PartialEq)]
pub struct NewExecution {
    pub event: Option<NewTriggerEvent>,
    pub execution: Execution,
    pub job: NewJob,
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
    async fn list_executions(
        &self,
        org: OrgId,
        filter: &ExecutionFilter,
    ) -> Res<Vec<(Execution, String)>>;
    async fn list_step_runs(&self, execution: ExecutionId) -> Res<Vec<StepRun>>;
    /// Latest output per step key (succeeded, or `{"error":…}` for continued failures).
    async fn load_step_outputs(&self, execution: ExecutionId) -> Res<Map<String, Value>>;
    async fn insert_step_run(&self, org: OrgId, run: &StepRun) -> Res<()>;
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

/// A connection with its encrypted secrets.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredConnection {
    pub connection: Connection,
    pub ciphertext: Vec<u8>,
    pub key_id: String,
}

/// A pending OAuth authorization.
#[derive(Debug, Clone, PartialEq)]
pub struct OAuthState {
    pub state_hash: Vec<u8>,
    pub org_id: OrgId,
    pub connector: String,
    pub connection_id: Option<ConnectionId>,
    pub connection_name: String,
    pub config: Value,
    pub verifier_cipher: Option<Vec<u8>>,
    pub return_to: String,
    pub expires_at: DateTime<Utc>,
}

#[async_trait]
pub trait ConnectionRepo: Send + Sync {
    async fn insert_connection(&self, conn: &StoredConnection) -> Res<()>;
    async fn get_connection(&self, org: OrgId, id: ConnectionId) -> Res<Option<StoredConnection>>;
    async fn list_connections(&self, org: OrgId) -> Res<Vec<Connection>>;
    async fn update_connection(&self, conn: &StoredConnection) -> Res<()>;
    async fn set_connection_status(
        &self,
        id: ConnectionId,
        status: ConnectionStatus,
        now: DateTime<Utc>,
    ) -> Res<()>;
    async fn soft_delete_connection(
        &self,
        org: OrgId,
        id: ConnectionId,
        now: DateTime<Utc>,
    ) -> Res<()>;
    async fn insert_oauth_state(&self, state: &OAuthState, now: DateTime<Utc>) -> Res<()>;
    /// Removes and returns a state (single use) if it has not expired.
    async fn take_oauth_state(
        &self,
        state_hash: &[u8],
        now: DateTime<Utc>,
    ) -> Res<Option<OAuthState>>;
}

/// One side of a record link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordSide {
    pub connection_id: ConnectionId,
    pub object: String,
    pub record_id: String,
}

/// A link between a source record (side A) and a target record (side B).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordLink {
    pub id: LinkId,
    pub org_id: OrgId,
    pub link_set: String,
    pub a: RecordSide,
    pub b: RecordSide,
    pub b_hash: Option<Vec<u8>>,
    pub b_written_at: Option<DateTime<Utc>>,
}

#[async_trait]
pub trait LinkRepo: Send + Sync {
    async fn find_link_by_a(
        &self,
        org: OrgId,
        link_set: &str,
        a: &RecordSide,
    ) -> Res<Option<RecordLink>>;
    async fn find_link_by_b(
        &self,
        org: OrgId,
        link_set: &str,
        b: &RecordSide,
    ) -> Res<Option<RecordLink>>;
    /// Inserts or updates by side A. Fails with a conflict if side B is already linked to another A.
    async fn upsert_link(&self, link: &RecordLink, now: DateTime<Utc>) -> Res<()>;
    async fn delete_link(&self, id: LinkId) -> Res<()>;
    async fn acquire_lease(
        &self,
        org: OrgId,
        key: &[u8],
        holder: &str,
        until: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Res<bool>;
    async fn release_lease(&self, org: OrgId, key: &[u8], holder: &str) -> Res<()>;
}

/// Everything persistent, implemented by one MySQL adapter.
pub trait Store:
    OrgRepo
    + FlowRepo
    + TriggerRepo
    + JobQueue
    + ExecutionRepo
    + ConnectionRepo
    + LinkRepo
    + Send
    + Sync
{
}

impl<T> Store for T where
    T: OrgRepo
        + FlowRepo
        + TriggerRepo
        + JobQueue
        + ExecutionRepo
        + ConnectionRepo
        + LinkRepo
        + Send
        + Sync
{
}
