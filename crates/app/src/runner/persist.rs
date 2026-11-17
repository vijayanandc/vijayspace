//! Persisting a slice's progress: engine trace entries, checkpoints and the final status.

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use weft_domain::model::{Execution, ExecutionStatus, StepRun, StepStatus};
use weft_engine::{RunState, TraceEntry};

use crate::{
    AppError, Deps,
    ports::{Checkpoint, Finish},
};

/// Stores one engine trace entry (decision, delay, variable, stop) as a step run.
pub(super) async fn record_trace(
    deps: &Deps,
    exe: &Execution,
    entry: &TraceEntry,
    seq: &mut u32,
    now: DateTime<Utc>,
) -> Result<(), AppError> {
    let (step, kind, status, output, error, warnings) = match entry {
        TraceEntry::Decision {
            step,
            branch,
            warnings,
        } => (
            step,
            "decision",
            StepStatus::Succeeded,
            Some(json!({"branch": branch})),
            None,
            warnings.clone(),
        ),
        TraceEntry::VariablesSet { step, values } => (
            step,
            "set_variable",
            StepStatus::Succeeded,
            Some(Value::Object(values.clone())),
            None,
            Vec::new(),
        ),
        TraceEntry::DelayScheduled { step, until } => (
            step,
            "delay",
            StepStatus::Running,
            Some(json!({"until": until})),
            None,
            Vec::new(),
        ),
        TraceEntry::DelayCompleted { step } => (
            step,
            "delay",
            StepStatus::Succeeded,
            Some(json!({"completed": true})),
            None,
            Vec::new(),
        ),
        TraceEntry::StepFailed {
            step,
            code,
            message,
            continued,
        } => {
            let status = if *continued {
                StepStatus::Skipped
            } else {
                StepStatus::Failed
            };
            (
                step,
                "step",
                status,
                None,
                Some(json!({"code": code, "message": message})),
                Vec::new(),
            )
        }
        TraceEntry::Stopped {
            step,
            status,
            message,
        } => (
            step,
            "stop",
            StepStatus::Succeeded,
            Some(json!({"status": status, "message": message})),
            None,
            Vec::new(),
        ),
    };
    let run = StepRun {
        id: deps.new_step_run_id(),
        execution_id: exe.id,
        seq: *seq,
        step_key: step.clone(),
        kind: kind.to_owned(),
        attempt: 1,
        status,
        input: None,
        output,
        error,
        warnings,
        started_at: now,
        finished_at: Some(now),
    };
    *seq += 1;
    deps.store.insert_step_run(exe.org_id, &run).await
}

pub(super) async fn checkpoint(
    deps: &Deps,
    exe: &Execution,
    state: &RunState,
    status: ExecutionStatus,
    waiting_until: Option<DateTime<Utc>>,
) -> Result<(), AppError> {
    let now = deps.clock.now();
    deps.store
        .checkpoint(&Checkpoint {
            execution_id: exe.id,
            org_id: exe.org_id,
            status,
            run_state: to_value(state),
            waiting_until,
            now,
        })
        .await
}

pub(super) async fn finish(
    deps: &Deps,
    exe: &Execution,
    state: &RunState,
    status: ExecutionStatus,
    error: Option<(&'static str, String, Option<String>)>,
    now: DateTime<Utc>,
) -> Result<(), AppError> {
    let (error_code, error_message, failed_step_key) = match error {
        Some((code, message, step)) => (
            Some(code.to_owned()),
            Some(message.chars().take(2000).collect()),
            step,
        ),
        None => (None, None, None),
    };
    deps.store
        .finish_execution(&Finish {
            execution_id: exe.id,
            status,
            run_state: to_value(state),
            error_code,
            error_message,
            failed_step_key,
            now,
        })
        .await?;
    if status == ExecutionStatus::Failed {
        tracing::warn!(execution = %exe.id, flow = %exe.flow_id, "execution failed");
    }
    Ok(())
}

/// The run state as stored JSON.
pub(super) fn to_value(state: &RunState) -> Value {
    serde_json::to_value(state).unwrap_or(Value::Null)
}
