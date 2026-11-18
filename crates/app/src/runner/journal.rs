//! Writing a run's detailed log lines (`execution_logs`): HTTP calls of each attempt, requests held back by
//! limits, scheduled retries and runs that may have done a step twice. The log is diagnostic: a failure to
//! write it is logged and never fails the run.

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::json;
use weft_domain::{ids::ExecutionId, model::Execution};
use weft_engine::StepError;

use crate::{
    Deps,
    ports::{LogKind, LogLevel, LogRecord},
};

/// Appends `records` to the run's log (best effort).
pub(super) async fn save(deps: &Deps, exe: &Execution, mut records: Vec<LogRecord>) {
    if records.is_empty() {
        return;
    }
    for r in &mut records {
        r.execution_id = exe.id;
    }
    if let Err(e) = deps.store.append_logs(exe.org_id, exe.id, &records).await {
        tracing::warn!(execution = %exe.id, error = %e, "writing the run's log");
    }
}

/// A line for the run being saved (`save` fills in the run).
fn line(
    deps: &Deps,
    step: Option<&str>,
    attempt: Option<u16>,
    (level, kind): (LogLevel, LogKind),
    message: String,
    details: serde_json::Value,
    at: DateTime<Utc>,
) -> LogRecord {
    LogRecord {
        id: deps.ids.uuid(),
        execution_id: ExecutionId::from_uuid(uuid::Uuid::nil()),
        step_key: step.map(str::to_owned),
        attempt,
        level,
        kind,
        message,
        details: Some(details),
        at,
    }
}

fn stamp(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// A request the connection's limits held back: the step was not attempted and waits until `until`.
pub(super) fn held_back(
    deps: &Deps,
    step: &str,
    err: &StepError,
    until: DateTime<Utc>,
    now: DateTime<Utc>,
) -> LogRecord {
    let why = err
        .message
        .rsplit_once("connection.limit: ")
        .map_or(err.message.as_str(), |(_, why)| why);
    line(
        deps,
        Some(step),
        None,
        (LogLevel::Warn, LogKind::Limit),
        format!(
            "Held back by the connection's limits ({why}); the step waits and is not counted as an attempt"
        ),
        json!({"reason": why, "until": stamp(until)}),
        now,
    )
}

/// An attempt failed with a retryable error; the next one runs at `at`.
pub(super) fn retry_scheduled(
    deps: &Deps,
    step: &str,
    attempt: u16,
    err: &StepError,
    at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> LogRecord {
    line(
        deps,
        Some(step),
        Some(attempt),
        (LogLevel::Warn, LogKind::Retry),
        format!(
            "Attempt {attempt} failed ({}); attempt {} is scheduled",
            err.code,
            attempt.saturating_add(1)
        ),
        json!({"code": err.code, "retry_at": stamp(at), "next_attempt": attempt.saturating_add(1)}),
        now,
    )
}

/// A worker stopped after calling an app and before saving the answer: the step may have run twice.
pub(super) fn unknown_outcome(deps: &Deps, now: DateTime<Utc>) -> LogRecord {
    line(
        deps,
        None,
        None,
        (LogLevel::Warn, LogKind::Event),
        "A worker stopped while a step was calling an app; the step runs again and may have run twice".to_owned(),
        json!({"possible_duplicate": true}),
        now,
    )
}
