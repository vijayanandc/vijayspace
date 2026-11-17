//! `run_execution`: one slice of work on one execution (until it finishes, sleeps or yields).

use chrono::{DateTime, Duration, Utc};
use serde_json::{Map, Value, json};
use weft_domain::{
    error_codes,
    flow::StepKind,
    ids::ExecutionId,
    model::{Execution, ExecutionStatus, StepRun, StepStatus},
};
use weft_engine::{
    AfterResult, Directive, Facts, Outcome, Plan, RunState, StepResult, advance,
    continued_error_output, record_result,
};

use super::{
    action::{call_action, idempotency_key, step_error},
    persist::{checkpoint, finish, record_trace, to_value},
};
use crate::{
    AppError, Deps,
    ports::{Checkpoint, StepCompletion},
};

/// Actions per slice before yielding the worker to other executions.
const MAX_ACTIONS_PER_SLICE: u32 = 25;

/// What the worker does with the job afterwards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobOutcome {
    /// The execution is finished (or gone): delete the job.
    Done,
    /// Run the job again at this time (delay, retry backoff, or fairness yield).
    RescheduleAt(DateTime<Utc>),
}

fn state_of(exe: &Execution) -> Result<RunState, AppError> {
    serde_json::from_value(exe.run_state.clone())
        .map_err(|e| AppError::internal(format!("corrupt run state of {}: {e}", exe.id)))
}

/// Runs one slice of the execution.
pub async fn run_execution(deps: &Deps, id: ExecutionId) -> Result<JobOutcome, AppError> {
    let Some(exe) = deps.store.get_execution_system(id).await? else {
        return Ok(JobOutcome::Done);
    };
    if exe.status.is_terminal() {
        return Ok(JobOutcome::Done);
    }
    let now = deps.clock.now();
    if exe.cancel_requested {
        finish(
            deps,
            &exe,
            &state_of(&exe)?,
            ExecutionStatus::Cancelled,
            Some((
                error_codes::CANCELLED,
                "Cancelled by a user".to_owned(),
                None,
            )),
            now,
        )
        .await?;
        return Ok(JobOutcome::Done);
    }
    let version = deps
        .store
        .get_version(exe.flow_version_id)
        .await?
        .ok_or_else(|| AppError::internal(format!("version {} missing", exe.flow_version_id)))?;
    let plan = deps.plans.get(&version)?;
    if deps.store.mark_unknown_outcomes(exe.id, now).await? > 0 {
        // A previous worker crashed after calling an action but before recording its result.
        deps.store.mark_possible_duplicate(exe.id).await?;
    }
    deps.store.mark_started(exe.id, now).await?;
    let meta = json!({
        "flow": {"id": exe.flow_id, "version": version.version},
        "execution": {"id": exe.id, "mode": exe.mode, "started_at": exe.started_at.unwrap_or(now)},
    });
    let mut ctx = SliceCtx {
        outputs: deps.store.load_step_outputs(exe.id).await?,
        seq: deps.store.next_step_seq(exe.id).await?,
    };
    let mut state = state_of(&exe)?;
    let mut actions = 0;
    loop {
        if actions >= MAX_ACTIONS_PER_SLICE {
            checkpoint(deps, &exe, &state, ExecutionStatus::Queued, None).await?;
            return Ok(JobOutcome::RescheduleAt(deps.clock.now()));
        }
        let now = deps.clock.now();
        let facts = Facts {
            trigger: &exe.trigger_data,
            outputs: &ctx.outputs,
            meta: &meta,
            now,
        };
        let adv = advance(&plan, state, &facts)
            .map_err(|e| AppError::internal(format!("engine: {e}")))?;
        state = adv.state;
        for entry in &adv.trace {
            record_trace(deps, &exe, entry, &mut ctx.seq, now).await?;
        }
        match adv.directive {
            Directive::Finish(outcome) => {
                let (status, err) = match outcome {
                    Outcome::Succeeded => (ExecutionStatus::Succeeded, None),
                    Outcome::Failed {
                        code,
                        message,
                        step,
                    } => (ExecutionStatus::Failed, Some((leak(&code), message, step))),
                };
                finish(deps, &exe, &state, status, err, now).await?;
                return Ok(JobOutcome::Done);
            }
            Directive::Sleep { until, .. } => {
                checkpoint(deps, &exe, &state, ExecutionStatus::Waiting, Some(until)).await?;
                return Ok(JobOutcome::RescheduleAt(until));
            }
            Directive::RunAction {
                step,
                attempt,
                input,
            } => {
                actions += 1;
                let next =
                    run_action(deps, &exe, &plan, &mut ctx, state, step, attempt, input).await?;
                state = next.0;
                if let Some(job) = next.1 {
                    return Ok(job);
                }
            }
        }
    }
}

/// Mutable per-slice bookkeeping.
struct SliceCtx {
    outputs: Map<String, Value>,
    seq: u32,
}

/// Records the attempt, calls the action, applies the engine's decision and persists it.
/// Returns the new state and, when the slice must end, the job outcome.
#[allow(clippy::too_many_arguments)]
async fn run_action(
    deps: &Deps,
    exe: &Execution,
    plan: &Plan,
    ctx: &mut SliceCtx,
    state: RunState,
    step: String,
    attempt: u16,
    input: Value,
) -> Result<(RunState, Option<JobOutcome>), AppError> {
    let step_run_id = deps.new_step_run_id();
    let started = deps.clock.now();
    let run = StepRun {
        id: step_run_id,
        execution_id: exe.id,
        seq: ctx.seq,
        step_key: step.clone(),
        kind: "action".into(),
        attempt,
        status: StepStatus::Running,
        input: Some(input.clone()),
        output: None,
        error: None,
        warnings: Vec::new(),
        started_at: started,
        finished_at: None,
    };
    deps.store.insert_step_run(exe.org_id, &run).await?;
    ctx.seq += 1;
    let result = run_step(deps, exe, plan, &step, input).await;
    let done_at = deps.clock.now();
    let (next, after) = record_result(plan, state, &step, &result, done_at)
        .map_err(|e| AppError::internal(format!("engine: {e}")))?;
    let outcome = complete(
        deps,
        exe,
        &next,
        step_run_id,
        &step,
        &result,
        &after,
        done_at,
        &mut ctx.outputs,
    )
    .await?;
    Ok((next, outcome))
}

/// Error codes are `&'static str` in `AppError`; engine codes are owned strings. Map known codes.
fn leak(code: &str) -> &'static str {
    match code {
        c if c == error_codes::MAPPING_ERROR => error_codes::MAPPING_ERROR,
        c if c == error_codes::DELAY_INVALID_DATETIME => error_codes::DELAY_INVALID_DATETIME,
        c if c == error_codes::ENGINE_BUDGET_EXCEEDED => error_codes::ENGINE_BUDGET_EXCEEDED,
        c if c == error_codes::STOPPED_BY_FLOW => error_codes::STOPPED_BY_FLOW,
        c if c == error_codes::APP_UNAVAILABLE => error_codes::APP_UNAVAILABLE,
        c if c == error_codes::APP_RATE_LIMITED => error_codes::APP_RATE_LIMITED,
        c if c == error_codes::APP_INVALID_INPUT => error_codes::APP_INVALID_INPUT,
        c if c == error_codes::APP_NOT_FOUND => error_codes::APP_NOT_FOUND,
        c if c == error_codes::CONNECTION_NEEDS_REAUTH => error_codes::CONNECTION_NEEDS_REAUTH,
        _ => error_codes::APP_ERROR,
    }
}

async fn run_step(
    deps: &Deps,
    exe: &Execution,
    plan: &Plan,
    step: &str,
    input: Value,
) -> StepResult {
    let Some(StepKind::Action(action)) = plan.step(step).map(|s| &s.kind) else {
        return StepResult::Failed(step_error(&weft_connector_sdk::ConnectorError::permanent(
            "step is not an action",
        )));
    };
    let idem = idempotency_key(&exe.id.to_string(), step);
    match call_action(
        deps,
        exe.org_id,
        &action.connector,
        &action.action,
        action.connection_id.as_deref(),
        input,
        idem,
    )
    .await
    {
        Ok(output) => StepResult::Succeeded(output),
        Err(err) => {
            if matches!(err, weft_connector_sdk::ConnectorError::AuthExpired { .. })
                && let Some(Ok(cid)) = action.connection_id.as_deref().map(str::parse)
            {
                let _ = deps
                    .store
                    .set_connection_status(
                        cid,
                        weft_domain::model::ConnectionStatus::NeedsReauth,
                        deps.clock.now(),
                    )
                    .await;
            }
            StepResult::Failed(step_error(&err))
        }
    }
}

/// Persists the action result. Returns `Some(job outcome)` when this slice must end.
#[allow(clippy::too_many_arguments)]
async fn complete(
    deps: &Deps,
    exe: &Execution,
    state: &RunState,
    step_run_id: weft_domain::ids::StepRunId,
    step: &str,
    result: &StepResult,
    after: &AfterResult,
    now: DateTime<Utc>,
    outputs: &mut Map<String, Value>,
) -> Result<Option<JobOutcome>, AppError> {
    let (status, output, error) = match result {
        StepResult::Succeeded(out) => (StepStatus::Succeeded, Some(out.clone()), None),
        StepResult::Failed(err) => {
            let status = if matches!(after, AfterResult::RetryAt(_)) {
                StepStatus::RetryScheduled
            } else {
                StepStatus::Failed
            };
            (
                status,
                None,
                Some(serde_json::to_value(err).unwrap_or(Value::Null)),
            )
        }
    };
    let (exec_status, waiting_until, job) = match after {
        AfterResult::Continue | AfterResult::FailExecution(_) => {
            (ExecutionStatus::Running, None, None)
        }
        AfterResult::RetryAt(at) => (
            ExecutionStatus::Waiting,
            Some(*at),
            Some(JobOutcome::RescheduleAt(*at)),
        ),
    };
    let completion = StepCompletion {
        step_run_id,
        status,
        output: output.clone(),
        error,
        finished_at: now,
        execution: Checkpoint {
            execution_id: exe.id,
            org_id: exe.org_id,
            status: exec_status,
            run_state: to_value(state),
            waiting_until,
            now,
        },
        count_task: !matches!(after, AfterResult::RetryAt(_)),
    };
    deps.store.complete_step(&completion).await?;
    match (after, result) {
        (AfterResult::Continue, StepResult::Succeeded(out)) => {
            outputs.insert(step.to_owned(), out.clone());
            Ok(None)
        }
        (AfterResult::Continue, StepResult::Failed(err)) => {
            outputs.insert(step.to_owned(), continued_error_output(err));
            Ok(None)
        }
        (AfterResult::RetryAt(_), _) => Ok(job),
        (AfterResult::FailExecution(err), _) => {
            finish(
                deps,
                exe,
                state,
                ExecutionStatus::Failed,
                Some((leak(&err.code), err.message.clone(), Some(step.to_owned()))),
                now,
            )
            .await?;
            Ok(Some(JobOutcome::Done))
        }
    }
}

/// Delay used when a job fails for infrastructure reasons (database down…).
#[must_use]
pub fn infra_backoff(attempts: u32) -> Duration {
    Duration::seconds(i64::from(attempts.clamp(1, 10)) * 30)
}
