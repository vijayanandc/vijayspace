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
    calls::{self, CallLog},
    journal,
    persist::{checkpoint, finish, leak, record_trace, throttled, to_value},
};
use crate::{
    AppError, Deps,
    limits::RequestClass,
    ports::{Checkpoint, StepCompletion},
};

/// A run may wait (delays, chained delays, "until" dates) at most this long after it was queued.
const MAX_RUN_DAYS: i64 = crate::retention::RUN_DAYS as i64;

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
/// The compiled plan of a version. Versions are immutable: a cached plan saves loading and parsing the definition
/// on every slice of a run.
async fn plan_for(
    deps: &Deps,
    id: weft_domain::ids::FlowVersionId,
) -> Result<super::plan_cache::CachedPlan, AppError> {
    if let Some(hit) = deps.plans.cached(id) {
        return Ok(hit);
    }
    let version = deps
        .store
        .get_version(id)
        .await?
        .ok_or_else(|| AppError::internal(format!("version {id} missing")))?;
    Ok((deps.plans.get(&version)?, version.version))
}

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
    let (plan, version_number) = plan_for(deps, exe.flow_version_id).await?;
    if deps.store.mark_unknown_outcomes(exe.id, now).await? > 0 {
        // A previous worker crashed after calling an action but before recording its result.
        deps.store.mark_possible_duplicate(exe.id).await?;
        journal::save(deps, &exe, vec![journal::unknown_outcome(deps, now)]).await;
    }
    deps.store.mark_started(exe.id, now).await?;
    let meta = json!({
        "flow": {"id": exe.flow_id, "version": version_number},
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
            Directive::Sleep { until, step } => {
                // History (and with it the waiting run) is kept 30 days from the day the run was queued: a wait
                // past `RUN_DAYS` would end with the run expired. Fail now, visibly, instead.
                if until > exe.queued_at + chrono::Duration::days(MAX_RUN_DAYS) {
                    let why = format!(
                        "The delay ends after {until}, more than {MAX_RUN_DAYS} days after the run started; runs must finish within that"
                    );
                    let err = Some((error_codes::DELAY_TOO_LONG, why, Some(step)));
                    finish(deps, &exe, &state, ExecutionStatus::Failed, err, now).await?;
                    return Ok(JobOutcome::Done);
                }
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
    let calls = std::sync::Arc::new(CallLog::default());
    let result = calls::recording(calls.clone(), run_step(deps, exe, plan, &step, input)).await;
    let done_at = deps.clock.now();
    let mut lines = calls.records(deps, &step, attempt);
    if let StepResult::Failed(err) = &result
        && err.code == error_codes::CONNECTION_THROTTLED
    {
        // Our own limits held the request back: it was never sent, so it is not an attempt and
        // leaves no history row; the instruction that issued it is given back too, so parking
        // never uses up the execution's budget.
        let mut parked = state;
        parked.executed = parked.executed.saturating_sub(1);
        let at = done_at + crate::limits::park_delay(exe.id, err.retry_after_secs.unwrap_or(30));
        lines.push(journal::held_back(deps, &step, err, at, done_at));
        journal::save(deps, exe, lines).await;
        let job = throttled(deps, exe, &parked, step_run_id, at, done_at).await?;
        return Ok((parked, Some(job)));
    }
    let (next, after) = record_result(plan, state, &step, &result, done_at)
        .map_err(|e| AppError::internal(format!("engine: {e}")))?;
    if let (AfterResult::RetryAt(at), StepResult::Failed(err)) = (&after, &result) {
        lines.push(journal::retry_scheduled(
            deps, &step, attempt, err, *at, done_at,
        ));
    }
    journal::save(deps, exe, lines).await;
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
        if exe.mode == weft_domain::model::ExecutionMode::Backfill {
            RequestClass::Background
        } else {
            RequestClass::Live
        },
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
