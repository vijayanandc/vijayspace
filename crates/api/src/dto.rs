//! Response DTOs (the JSON shapes in `docs/api/README.md`).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use weft_app::{
    Deps,
    flows::{FlowView, manage_urls},
    ports::FlowListRow,
};
use weft_domain::{
    flow::{FlowDefinition, TriggerNode},
    ids::{ExecutionId, FlowId, FlowVersionId},
    model::{Execution, ExecutionMode, ExecutionStatus, RegistrationKind, StepRun},
};

#[derive(Debug, Serialize)]
pub struct LastExecutionDto {
    pub id: ExecutionId,
    pub status: ExecutionStatus,
    pub queued_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct FlowSummaryDto {
    pub id: FlowId,
    pub name: String,
    pub description: Option<String>,
    pub is_enabled: bool,
    pub trigger_type: &'static str,
    pub published_version: Option<u32>,
    pub has_unpublished_changes: bool,
    pub draft_revision: u32,
    pub updated_at: DateTime<Utc>,
    pub last_execution: Option<LastExecutionDto>,
}

pub fn trigger_type(def: &FlowDefinition) -> &'static str {
    match def.trigger {
        TriggerNode::Webhook(_) => "webhook",
        TriggerNode::Schedule(_) => "schedule",
        TriggerNode::App(_) => "app",
        TriggerNode::Manual => "manual",
    }
}

impl FlowSummaryDto {
    pub fn from_row(row: &FlowListRow, has_unpublished_changes: bool) -> Self {
        let f = &row.flow;
        Self {
            id: f.id,
            name: f.name.clone(),
            description: f.description.clone(),
            is_enabled: f.is_enabled,
            trigger_type: trigger_type(&f.draft),
            published_version: f.published_version,
            has_unpublished_changes,
            draft_revision: f.draft_revision,
            updated_at: f.updated_at,
            last_execution: row
                .last_execution
                .map(|(id, status, queued_at)| LastExecutionDto {
                    id,
                    status,
                    queued_at,
                }),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct TriggerInfoDto {
    pub kind: RegistrationKind,
    pub token: String,
    pub webhook_url: String,
    pub app_event_url: String,
    /// The verify secret the app must send (Appblocks `X-Weft-Key`, webhook secret header).
    pub verify_token: String,
    pub is_active: bool,
    pub listening_until: Option<DateTime<Utc>>,
    pub next_fire_at: Option<DateTime<Utc>>,
    pub sample: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct FlowDto {
    pub id: FlowId,
    pub name: String,
    pub description: Option<String>,
    pub is_enabled: bool,
    pub trigger_type: &'static str,
    pub published_version: Option<u32>,
    pub published_version_id: Option<FlowVersionId>,
    pub has_unpublished_changes: bool,
    pub draft_revision: u32,
    pub draft: FlowDefinition,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub trigger: TriggerInfoDto,
}

impl FlowDto {
    pub fn new(deps: &Deps, view: &FlowView, has_unpublished_changes: bool) -> Self {
        let f = &view.flow;
        let reg = &view.registration;
        Self {
            id: f.id,
            name: f.name.clone(),
            description: f.description.clone(),
            is_enabled: f.is_enabled,
            trigger_type: trigger_type(&f.draft),
            published_version: f.published_version,
            published_version_id: f.published_version_id,
            has_unpublished_changes,
            draft_revision: f.draft_revision,
            draft: f.draft.clone(),
            created_at: f.created_at,
            updated_at: f.updated_at,
            trigger: TriggerInfoDto {
                kind: manage_urls::display_kind(deps, view),
                token: reg.token.clone(),
                webhook_url: manage_urls::webhook_url(deps, reg),
                app_event_url: manage_urls::app_event_url(deps, reg),
                verify_token: reg.secret.clone(),
                is_active: reg.is_active,
                listening_until: reg.listening_until,
                next_fire_at: reg.next_fire_at,
                sample: reg.sample.clone(),
            },
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ExecutionSummaryDto {
    pub id: ExecutionId,
    pub flow_id: FlowId,
    pub flow_name: String,
    pub status: ExecutionStatus,
    pub mode: ExecutionMode,
    pub queued_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub waiting_until: Option<DateTime<Utc>>,
    pub task_count: u32,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub failed_step_key: Option<String>,
    pub possible_duplicate: bool,
    pub rerun_of: Option<ExecutionId>,
}

impl ExecutionSummaryDto {
    pub fn new(e: &Execution, flow_name: String) -> Self {
        Self {
            id: e.id,
            flow_id: e.flow_id,
            flow_name,
            status: e.status,
            mode: e.mode,
            queued_at: e.queued_at,
            started_at: e.started_at,
            finished_at: e.finished_at,
            waiting_until: e.waiting_until,
            task_count: e.task_count,
            error_code: e.error_code.clone(),
            error_message: e.error_message.clone(),
            failed_step_key: e.failed_step_key.clone(),
            possible_duplicate: e.possible_duplicate,
            rerun_of: e.rerun_of,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ExecutionDetailDto {
    #[serde(flatten)]
    pub summary: ExecutionSummaryDto,
    pub trigger_data: Value,
    pub steps: Vec<StepRun>,
}

#[derive(Debug, Serialize)]
pub struct VersionDto {
    pub id: FlowVersionId,
    pub version: Option<u32>,
    pub created_at: DateTime<Utc>,
}

/// `{connection_id?, input}` for options/fields endpoints.
#[derive(Debug, Deserialize)]
pub struct DynamicRequest {
    #[serde(default)]
    pub connection_id: Option<String>,
    #[serde(default)]
    pub input: Value,
}
