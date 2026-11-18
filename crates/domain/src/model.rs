//! Entities shared by the use cases, the database adapter and the API.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    Timestamp,
    flow::FlowDefinition,
    ids::{
        ConnectionId, ExecutionId, FlowId, FlowVersionId, OrgId, RegistrationId, StepRunId,
        TriggerEventId,
    },
};

/// A tenant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Org {
    pub id: OrgId,
    pub name: String,
    pub timezone: String,
    /// How long finished runs are kept; `None` = the server default.
    #[serde(default)]
    pub retention_days: Option<u32>,
    pub created_at: Timestamp,
}

/// A flow with its editable draft.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Flow {
    pub id: FlowId,
    pub org_id: OrgId,
    pub name: String,
    pub description: Option<String>,
    pub is_enabled: bool,
    pub draft: FlowDefinition,
    pub draft_revision: u32,
    pub published_version_id: Option<FlowVersionId>,
    pub published_version: Option<u32>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// Kind of stored version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionKind {
    Published,
    TestSnapshot,
}

/// An immutable snapshot of a definition; executions pin one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowVersion {
    pub id: FlowVersionId,
    pub org_id: OrgId,
    pub flow_id: FlowId,
    pub kind: VersionKind,
    pub version: Option<u32>,
    pub definition: FlowDefinition,
    pub created_at: Timestamp,
}

/// Execution lifecycle (`docs/plan/04-workflow-engine.md` §1.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    Queued,
    Running,
    Waiting,
    Succeeded,
    Failed,
    Cancelled,
}

impl ExecutionStatus {
    /// No further work will happen.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

/// Why an execution was started.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    Live,
    Test,
    Manual,
    Rerun,
    Backfill,
}

/// One run of a flow version.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Execution {
    pub id: ExecutionId,
    pub org_id: OrgId,
    pub flow_id: FlowId,
    pub flow_version_id: FlowVersionId,
    pub trigger_event_id: Option<TriggerEventId>,
    pub mode: ExecutionMode,
    pub rerun_of: Option<ExecutionId>,
    pub status: ExecutionStatus,
    /// Trigger output available to templates as `trigger`.
    pub trigger_data: Value,
    /// Serialized engine `RunState`.
    pub run_state: Value,
    pub waiting_until: Option<Timestamp>,
    pub task_count: u32,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub failed_step_key: Option<String>,
    pub possible_duplicate: bool,
    pub cancel_requested: bool,
    pub queued_at: Timestamp,
    pub started_at: Option<Timestamp>,
    pub finished_at: Option<Timestamp>,
}

/// Status of one step attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Running,
    Succeeded,
    Failed,
    RetryScheduled,
    Skipped,
    UnknownOutcome,
}

/// The record of one step attempt (or one logic decision) inside an execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepRun {
    pub id: StepRunId,
    pub execution_id: ExecutionId,
    pub seq: u32,
    pub step_key: String,
    pub kind: String,
    pub attempt: u16,
    pub status: StepStatus,
    pub input: Option<Value>,
    pub output: Option<Value>,
    pub error: Option<Value>,
    pub warnings: Vec<String>,
    pub started_at: Timestamp,
    pub finished_at: Option<Timestamp>,
}

/// Connection health.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionStatus {
    Active,
    NeedsReauth,
    Error,
}

/// Stored credentials for one connector (secrets are encrypted separately and never exposed).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Connection {
    pub id: ConnectionId,
    pub org_id: OrgId,
    pub connector: String,
    pub name: String,
    /// Non-secret settings: base URL, data centre, API domain, region, account display info.
    pub config: Value,
    pub status: ConnectionStatus,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// How a flow is triggered at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistrationKind {
    Webhook,
    Schedule,
    Poll,
    Instant,
    Manual,
}

/// The runtime state of a flow's trigger (one per flow).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TriggerRegistration {
    pub id: RegistrationId,
    pub org_id: OrgId,
    pub flow_id: FlowId,
    pub kind: RegistrationKind,
    /// URL capability for `/hooks/{token}` and `/hooks/app/{token}`.
    pub token: String,
    /// Verify token for app events (Zoho `token`, Appblocks `X-Weft-Key`, Autodesk hook attribute).
    pub secret: String,
    pub is_active: bool,
    pub listening_until: Option<Timestamp>,
    pub next_fire_at: Option<Timestamp>,
    /// Poll cursor, last fired slot, and similar trigger-specific state.
    pub state: Value,
    pub sample: Option<Value>,
}
