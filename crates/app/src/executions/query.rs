//! Use cases: list executions, execution detail, usage.

use chrono::Datelike as _;
use weft_domain::{
    error_codes,
    ids::ExecutionId,
    model::{Execution, StepRun},
};

use crate::tenancy::Actor;
use crate::{AppError, Deps, ports::ExecutionFilter};
use weft_domain::permissions::Permission;

/// One page of executions with the flow name of each.
#[derive(Debug, Clone)]
pub struct ExecutionPage {
    pub items: Vec<(Execution, String)>,
    pub next: Option<(chrono::DateTime<chrono::Utc>, ExecutionId)>,
}

pub async fn list_executions(
    deps: &Deps,
    actor: &Actor,
    mut filter: ExecutionFilter,
) -> Result<ExecutionPage, AppError> {
    actor.require(Permission::ExecutionView)?;
    let org = actor.org;
    filter.limit = filter.limit.clamp(1, 200);
    let items = deps.store.list_executions(org, &filter).await?;
    let next = if items.len() == filter.limit as usize {
        items.last().map(|(e, _)| (e.queued_at, e.id))
    } else {
        None
    };
    Ok(ExecutionPage { items, next })
}

pub async fn get_execution(
    deps: &Deps,
    actor: &Actor,
    id: ExecutionId,
) -> Result<(Execution, Vec<StepRun>), AppError> {
    actor.require(Permission::ExecutionView)?;
    let org = actor.org;
    let exe = deps.store.get_execution(org, id).await?.ok_or_else(|| {
        AppError::not_found(error_codes::EXECUTION_NOT_FOUND, "Execution not found")
    })?;
    let steps = deps.store.list_step_runs(id).await?;
    if actor.sees_payloads() {
        return Ok((exe, steps));
    }
    Ok(redacted(exe, steps))
}

/// A run as viewers see it: statuses, timings and error codes, without any data.
fn redacted(mut exe: Execution, mut steps: Vec<StepRun>) -> (Execution, Vec<StepRun>) {
    exe.trigger_data = serde_json::Value::Null;
    exe.run_state = serde_json::Value::Null;
    for step in &mut steps {
        step.input = None;
        step.output = None;
        step.error = step
            .error
            .as_ref()
            .and_then(|e| e.get("code"))
            .map(|code| serde_json::json!({"code": code}));
    }
    (exe, steps)
}

/// `(period, tasks_used, executions)` for the current month.
pub async fn usage(deps: &Deps, actor: &Actor) -> Result<(String, u32, u32), AppError> {
    actor.require(Permission::FlowView)?;
    let org = actor.org;
    let now = deps.clock.now();
    let period = format!("{:04}-{:02}", now.year(), now.month());
    let (tasks, executions) = deps.store.usage(org, &period).await?;
    Ok((period, tasks, executions))
}
