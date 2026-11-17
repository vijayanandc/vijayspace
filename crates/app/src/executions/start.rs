//! Building a new execution together with the job that runs it.

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use weft_domain::{
    ids::{ExecutionId, FlowId, FlowVersionId, OrgId},
    model::{Execution, ExecutionMode, ExecutionStatus},
};
use weft_engine::RunState;

use crate::{
    Deps,
    ports::{NewExecution, NewJob, NewTriggerEvent},
};

/// Queue for a mode: test runs are kept apart so they are not stuck behind live traffic.
#[must_use]
pub fn queue_for(mode: ExecutionMode) -> &'static str {
    match mode {
        ExecutionMode::Test => "executions_test",
        ExecutionMode::Live | ExecutionMode::Manual | ExecutionMode::Rerun => "executions",
        ExecutionMode::Backfill => "sync_bulk",
    }
}

/// The run job for an execution.
#[must_use]
pub fn run_job(
    deps: &Deps,
    org: OrgId,
    id: ExecutionId,
    mode: ExecutionMode,
    run_at: DateTime<Utc>,
) -> NewJob {
    NewJob {
        id: deps.new_job_id(),
        queue: queue_for(mode).to_owned(),
        kind: "run_execution".into(),
        org_id: Some(org),
        payload: json!({"execution_id": id}),
        run_at,
        dedupe_key: Some(format!("exe:{id}")),
    }
}

/// A queued execution plus its job (and the trigger event that started it, if any).
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn new_execution(
    deps: &Deps,
    org: OrgId,
    flow: FlowId,
    version: FlowVersionId,
    mode: ExecutionMode,
    trigger_data: Value,
    event: Option<NewTriggerEvent>,
    run_at: DateTime<Utc>,
    rerun_of: Option<ExecutionId>,
) -> NewExecution {
    let now = deps.clock.now();
    let id = deps.new_execution_id();
    let execution = Execution {
        id,
        org_id: org,
        flow_id: flow,
        flow_version_id: version,
        trigger_event_id: event.as_ref().map(|e| e.id),
        mode,
        rerun_of,
        status: ExecutionStatus::Queued,
        trigger_data,
        run_state: serde_json::to_value(RunState::default()).unwrap_or(Value::Null),
        waiting_until: None,
        task_count: 0,
        error_code: None,
        error_message: None,
        failed_step_key: None,
        possible_duplicate: false,
        cancel_requested: false,
        queued_at: now,
        started_at: None,
        finished_at: None,
    };
    NewExecution {
        event,
        job: run_job(deps, org, id, mode, run_at),
        execution,
    }
}
