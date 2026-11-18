//! Use cases: retry from the failed step, re-run on the latest version, cancel.

use chrono::{DateTime, Duration, Utc};
use serde_json::Value;
use weft_domain::{
    error_codes,
    ids::{ExecutionId, OrgId},
    model::{ExecutionMode, ExecutionStatus},
};
use weft_engine::RunState;

use super::start::{new_execution, run_job};
use crate::tenancy::Actor;
use crate::{AppError, Deps, retention};
use weft_domain::permissions::Permission;

async fn load(
    deps: &Deps,
    org: OrgId,
    id: ExecutionId,
) -> Result<weft_domain::model::Execution, AppError> {
    deps.store
        .get_execution(org, id)
        .await?
        .ok_or_else(|| AppError::not_found(error_codes::EXECUTION_NOT_FOUND, "Execution not found"))
}

/// A retry resumes from the run's step history, so it is refused once the retention sweep may be
/// deleting that history (with an hour's margin, as a sweep can be part-way through a batch).
/// A re-run does not need the history and is always allowed.
async fn ensure_history_kept(
    deps: &Deps,
    org: OrgId,
    queued_at: DateTime<Utc>,
    finished_at: Option<DateTime<Utc>>,
) -> Result<(), AppError> {
    // Run history goes with the day a run was queued: a retried run must have time to finish before that.
    let platform_cutoff = deps.clock.now() - Duration::days(i64::from(retention::RUN_DAYS));
    if queued_at < platform_cutoff {
        return Err(AppError::conflict(
            error_codes::EXECUTION_NOT_RETRYABLE,
            "This run started 28 days ago or more and its history is about to be removed; re-run it instead",
        ));
    }
    let Some(finished_at) = finished_at else {
        return Ok(());
    };
    let Some(org_row) = deps.store.get_org(org).await? else {
        return Ok(());
    };
    let cutoff = retention::history_cutoff(deps, &org_row, deps.clock.now()) + Duration::hours(1);
    if finished_at < cutoff {
        return Err(AppError::conflict(
            error_codes::EXECUTION_NOT_RETRYABLE,
            "This run's history is past the org's retention period and is being removed; re-run it instead",
        ));
    }
    Ok(())
}

/// Resumes a failed execution at the step that failed, on the same pinned version.
pub async fn retry_execution(deps: &Deps, actor: &Actor, id: ExecutionId) -> Result<(), AppError> {
    actor.require(Permission::ExecutionRerun)?;
    let org = actor.org;
    let exe = load(deps, org, id).await?;
    if exe.status != ExecutionStatus::Failed {
        return Err(AppError::conflict(
            error_codes::EXECUTION_NOT_RETRYABLE,
            "Only failed executions can be retried",
        ));
    }
    ensure_history_kept(deps, org, exe.queued_at, exe.finished_at).await?;
    let mut state: RunState = serde_json::from_value(exe.run_state.clone()).unwrap_or_default();
    if let Some(step) = &exe.failed_step_key {
        state.attempts.remove(step);
    }
    let job = run_job(deps, org, id, exe.mode, deps.clock.now());
    deps.store
        .requeue_execution(
            id,
            &serde_json::to_value(state).unwrap_or(Value::Null),
            &job,
            deps.clock.now(),
        )
        .await?;
    deps.wake.notify_one();
    Ok(())
}

/// Starts a new execution with the same trigger data on the latest published version.
pub async fn rerun_execution(
    deps: &Deps,
    actor: &Actor,
    id: ExecutionId,
) -> Result<ExecutionId, AppError> {
    actor.require(Permission::ExecutionRerun)?;
    let org = actor.org;
    let exe = load(deps, org, id).await?;
    let flow = deps
        .store
        .get_flow(org, exe.flow_id)
        .await?
        .ok_or_else(|| AppError::not_found(error_codes::FLOW_NOT_FOUND, "The flow was deleted"))?;
    let version = match (exe.mode, flow.published_version_id) {
        (ExecutionMode::Test, _) | (_, None) => exe.flow_version_id,
        (_, Some(v)) => v,
    };
    let mode = if exe.mode == ExecutionMode::Test {
        ExecutionMode::Test
    } else {
        ExecutionMode::Rerun
    };
    let item = new_execution(
        deps,
        org,
        exe.flow_id,
        version,
        mode,
        exe.trigger_data.clone(),
        None,
        deps.clock.now(),
        Some(id),
    );
    let new_id = item.execution.id;
    deps.store
        .create_executions(org, exe.flow_id, &[item], deps.clock.now())
        .await?;
    deps.wake.notify_one();
    Ok(new_id)
}

/// Cancels a queued or waiting execution now; a running one stops at its next checkpoint.
pub async fn cancel_execution(deps: &Deps, actor: &Actor, id: ExecutionId) -> Result<(), AppError> {
    actor.require(Permission::ExecutionRerun)?;
    let org = actor.org;
    let exe = load(deps, org, id).await?;
    if exe.status.is_terminal() {
        return Err(AppError::conflict(
            error_codes::EXECUTION_NOT_RETRYABLE,
            "The execution has already finished",
        ));
    }
    deps.store.request_cancel(org, id, deps.clock.now()).await?;
    // Waiting/queued executions are finished right away by running their job now.
    let job = run_job(deps, org, id, exe.mode, deps.clock.now());
    deps.store.delete_jobs_for_execution(id).await?;
    deps.store.enqueue(&job, deps.clock.now()).await?;
    deps.wake.notify_one();
    Ok(())
}
