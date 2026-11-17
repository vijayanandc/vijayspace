//! Use cases: list executions, execution detail, usage.

use chrono::Datelike as _;
use weft_domain::{
    error_codes,
    ids::{ExecutionId, OrgId},
    model::{Execution, StepRun},
};

use crate::{AppError, Deps, ports::ExecutionFilter};

/// One page of executions with the flow name of each.
#[derive(Debug, Clone)]
pub struct ExecutionPage {
    pub items: Vec<(Execution, String)>,
    pub next: Option<(chrono::DateTime<chrono::Utc>, ExecutionId)>,
}

pub async fn list_executions(
    deps: &Deps,
    org: OrgId,
    mut filter: ExecutionFilter,
) -> Result<ExecutionPage, AppError> {
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
    org: OrgId,
    id: ExecutionId,
) -> Result<(Execution, Vec<StepRun>), AppError> {
    let exe = deps.store.get_execution(org, id).await?.ok_or_else(|| {
        AppError::not_found(error_codes::EXECUTION_NOT_FOUND, "Execution not found")
    })?;
    let steps = deps.store.list_step_runs(id).await?;
    Ok((exe, steps))
}

/// `(period, tasks_used, executions)` for the current month.
pub async fn usage(deps: &Deps, org: OrgId) -> Result<(String, u32, u32), AppError> {
    let now = deps.clock.now();
    let period = format!("{:04}-{:02}", now.year(), now.month());
    let (tasks, executions) = deps.store.usage(org, &period).await?;
    Ok((period, tasks, executions))
}
