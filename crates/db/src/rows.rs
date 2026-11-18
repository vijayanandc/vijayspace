//! Row structs (one per table) and their conversion into domain types.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{FromRow, types::Json};
use uuid::Uuid;
use weft_app::{
    AppError,
    ports::{StoredConnection, Subscription},
};
use weft_domain::{
    ids::{
        ConnectionId, ExecutionId, FlowId, FlowVersionId, OrgId, RegistrationId, StepRunId,
        SubscriptionId, TriggerEventId,
    },
    model::{Connection, Execution, Flow, FlowVersion, Org, StepRun, TriggerRegistration},
};

use crate::codec::{from_json, parse_enum};

#[derive(Debug, FromRow)]
pub(crate) struct OrgRow {
    pub id: Uuid,
    pub name: String,
    pub timezone: String,
    pub retention_days: Option<u32>,
    pub created_at: DateTime<Utc>,
}

impl From<OrgRow> for Org {
    fn from(r: OrgRow) -> Self {
        Self {
            id: OrgId::from_uuid(r.id),
            name: r.name,
            timezone: r.timezone,
            retention_days: r.retention_days,
            created_at: r.created_at,
        }
    }
}

#[derive(Debug, FromRow)]
pub(crate) struct FlowRow {
    pub id: Uuid,
    pub org_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub is_enabled: bool,
    pub draft_definition: Json<Value>,
    pub draft_revision: u32,
    pub published_version_id: Option<Uuid>,
    pub published_version: Option<u32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TryFrom<FlowRow> for Flow {
    type Error = AppError;
    fn try_from(r: FlowRow) -> Result<Self, AppError> {
        Ok(Self {
            id: FlowId::from_uuid(r.id),
            org_id: OrgId::from_uuid(r.org_id),
            name: r.name,
            description: r.description,
            is_enabled: r.is_enabled,
            draft: from_json(r.draft_definition.0, "flow draft")?,
            draft_revision: r.draft_revision,
            published_version_id: r.published_version_id.map(FlowVersionId::from_uuid),
            published_version: r.published_version,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
    }
}

#[derive(Debug, FromRow)]
pub(crate) struct VersionRow {
    pub id: Uuid,
    pub org_id: Uuid,
    pub flow_id: Uuid,
    pub kind: String,
    pub version: Option<u32>,
    pub definition: Json<Value>,
    pub created_at: DateTime<Utc>,
}

impl TryFrom<VersionRow> for FlowVersion {
    type Error = AppError;
    fn try_from(r: VersionRow) -> Result<Self, AppError> {
        Ok(Self {
            id: FlowVersionId::from_uuid(r.id),
            org_id: OrgId::from_uuid(r.org_id),
            flow_id: FlowId::from_uuid(r.flow_id),
            kind: parse_enum(&r.kind)?,
            version: r.version,
            definition: from_json(r.definition.0, "flow version")?,
            created_at: r.created_at,
        })
    }
}

#[derive(Debug, FromRow)]
pub(crate) struct RegistrationRow {
    pub id: Uuid,
    pub org_id: Uuid,
    pub flow_id: Uuid,
    pub kind: String,
    pub token: String,
    pub secret: String,
    pub is_active: bool,
    pub listening_until: Option<DateTime<Utc>>,
    pub next_fire_at: Option<DateTime<Utc>>,
    pub state: Json<Value>,
    pub sample: Option<Json<Value>>,
}

impl TryFrom<RegistrationRow> for TriggerRegistration {
    type Error = AppError;
    fn try_from(r: RegistrationRow) -> Result<Self, AppError> {
        Ok(Self {
            id: RegistrationId::from_uuid(r.id),
            org_id: OrgId::from_uuid(r.org_id),
            flow_id: FlowId::from_uuid(r.flow_id),
            kind: parse_enum(&r.kind)?,
            token: r.token,
            secret: r.secret,
            is_active: r.is_active,
            listening_until: r.listening_until,
            next_fire_at: r.next_fire_at,
            state: r.state.0,
            sample: r.sample.map(|s| s.0),
        })
    }
}

#[derive(Debug, FromRow)]
pub(crate) struct SubscriptionRow {
    pub id: Uuid,
    pub org_id: Uuid,
    pub registration_id: Uuid,
    pub connection_id: Uuid,
    pub connector_key: String,
    pub external_id: String,
    pub data: Json<Value>,
    pub status: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub renew_after: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

impl From<SubscriptionRow> for Subscription {
    fn from(r: SubscriptionRow) -> Self {
        Self {
            id: SubscriptionId::from_uuid(r.id),
            org_id: OrgId::from_uuid(r.org_id),
            registration_id: RegistrationId::from_uuid(r.registration_id),
            connection_id: ConnectionId::from_uuid(r.connection_id),
            connector: r.connector_key,
            external_id: r.external_id,
            data: r.data.0,
            status: r.status,
            expires_at: r.expires_at,
            renew_after: r.renew_after,
            last_error: r.last_error,
        }
    }
}

#[derive(Debug, FromRow)]
pub(crate) struct ExecutionRow {
    pub id: Uuid,
    pub org_id: Uuid,
    pub flow_id: Uuid,
    pub flow_version_id: Uuid,
    pub trigger_event_id: Option<Uuid>,
    pub mode: String,
    pub rerun_of: Option<Uuid>,
    pub status: String,
    pub trigger_data: Json<Value>,
    pub run_state: Json<Value>,
    pub waiting_until: Option<DateTime<Utc>>,
    pub task_count: u32,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub failed_step_key: Option<String>,
    pub possible_duplicate: bool,
    pub cancel_requested: bool,
    pub queued_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

impl TryFrom<ExecutionRow> for Execution {
    type Error = AppError;
    fn try_from(r: ExecutionRow) -> Result<Self, AppError> {
        Ok(Self {
            id: ExecutionId::from_uuid(r.id),
            org_id: OrgId::from_uuid(r.org_id),
            flow_id: FlowId::from_uuid(r.flow_id),
            flow_version_id: FlowVersionId::from_uuid(r.flow_version_id),
            trigger_event_id: r.trigger_event_id.map(TriggerEventId::from_uuid),
            mode: parse_enum(&r.mode)?,
            rerun_of: r.rerun_of.map(ExecutionId::from_uuid),
            status: parse_enum(&r.status)?,
            trigger_data: r.trigger_data.0,
            run_state: r.run_state.0,
            waiting_until: r.waiting_until,
            task_count: r.task_count,
            error_code: r.error_code,
            error_message: r.error_message,
            failed_step_key: r.failed_step_key,
            possible_duplicate: r.possible_duplicate,
            cancel_requested: r.cancel_requested,
            queued_at: r.queued_at,
            started_at: r.started_at,
            finished_at: r.finished_at,
        })
    }
}

#[derive(Debug, FromRow)]
pub(crate) struct StepRunRow {
    pub id: Uuid,
    pub execution_id: Uuid,
    pub seq: u32,
    pub step_key: String,
    pub kind: String,
    pub attempt: u16,
    pub status: String,
    pub input: Option<Json<Value>>,
    pub output: Option<Json<Value>>,
    pub error: Option<Json<Value>>,
    pub warnings: Option<Json<Vec<String>>>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

impl TryFrom<StepRunRow> for StepRun {
    type Error = AppError;
    fn try_from(r: StepRunRow) -> Result<Self, AppError> {
        Ok(Self {
            id: StepRunId::from_uuid(r.id),
            execution_id: ExecutionId::from_uuid(r.execution_id),
            seq: r.seq,
            step_key: r.step_key,
            kind: r.kind,
            attempt: r.attempt,
            status: parse_enum(&r.status)?,
            input: r.input.map(|j| j.0),
            output: r.output.map(|j| j.0),
            error: r.error.map(|j| j.0),
            warnings: r.warnings.map(|j| j.0).unwrap_or_default(),
            started_at: r.started_at,
            finished_at: r.finished_at,
        })
    }
}

#[derive(Debug, FromRow)]
pub(crate) struct ConnectionRow {
    pub id: Uuid,
    pub org_id: Uuid,
    pub connector_key: String,
    pub name: String,
    pub config: Json<Value>,
    pub secret_ciphertext: Vec<u8>,
    pub secret_key_id: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TryFrom<ConnectionRow> for StoredConnection {
    type Error = AppError;
    fn try_from(r: ConnectionRow) -> Result<Self, AppError> {
        Ok(Self {
            connection: Connection {
                id: ConnectionId::from_uuid(r.id),
                org_id: OrgId::from_uuid(r.org_id),
                connector: r.connector_key,
                name: r.name,
                config: r.config.0,
                status: parse_enum(&r.status)?,
                created_at: r.created_at,
                updated_at: r.updated_at,
            },
            ciphertext: r.secret_ciphertext,
            key_id: r.secret_key_id,
        })
    }
}
