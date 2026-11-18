//! Use case: the detailed log of a run, one chronological list built from the run itself (queued, started,
//! finished), its step runs (each attempt, decision, delay, …) and the stored log lines (every HTTP call, requests
//! held back by limits, scheduled retries). Viewers get statuses, codes and timings without data: no error
//! messages, warnings or variable names, and HTTP calls show the app's address only.

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use weft_domain::{
    error_codes,
    ids::ExecutionId,
    model::{Execution, ExecutionStatus, StepRun, StepStatus},
    permissions::Permission,
};

use crate::{
    AppError, Deps,
    ports::{LogKind, LogLevel, LogRecord},
    runner::calls::viewer_details,
    tenancy::Actor,
};

/// Stored lines read per run at most (a run makes at most 50 logged calls per attempt).
const MAX_LINES: u32 = 2000;

/// One line of a run's log.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LogEntry {
    pub at: DateTime<Utc>,
    pub level: LogLevel,
    pub kind: LogKind,
    pub step_key: Option<String>,
    pub attempt: Option<u16>,
    pub message: String,
    pub details: Option<Value>,
}

/// A run's log, oldest first. `truncated`: more stored lines exist than were read.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExecutionLog {
    pub entries: Vec<LogEntry>,
    pub truncated: bool,
}

pub async fn execution_log(
    deps: &Deps,
    actor: &Actor,
    id: ExecutionId,
) -> Result<ExecutionLog, AppError> {
    actor.require(Permission::ExecutionView)?;
    let exe = deps
        .store
        .get_execution(actor.org, id)
        .await?
        .ok_or_else(|| {
            AppError::not_found(error_codes::EXECUTION_NOT_FOUND, "Execution not found")
        })?;
    let steps = deps.store.list_step_runs(id).await?;
    let records = deps.store.list_logs(actor.org, id, MAX_LINES + 1).await?;
    let truncated = records.len() > MAX_LINES as usize;
    let records = records.into_iter().take(MAX_LINES as usize).collect();
    Ok(ExecutionLog {
        entries: timeline(&exe, &steps, records, actor.sees_payloads()),
        truncated,
    })
}

/// Order of lines at the same instant: the run's start, then step runs in sequence (each: start, its calls,
/// its end, its retry note), lines of no step run, then the run's end.
type SortKey = (DateTime<Utc>, i64, u8, usize);

struct Builder {
    lines: Vec<(SortKey, LogEntry)>,
    full: bool,
}

impl Builder {
    fn push(&mut self, key: (DateTime<Utc>, i64, u8), entry: LogEntry) {
        let n = self.lines.len();
        self.lines.push(((key.0, key.1, key.2, n), entry));
    }
}

fn entry(at: DateTime<Utc>, level: LogLevel, kind: LogKind, message: String) -> LogEntry {
    LogEntry {
        at,
        level,
        kind,
        step_key: None,
        attempt: None,
        message,
        details: None,
    }
}

/// Builds the log (pure). `full`: the reader may see data (not a viewer).
#[must_use]
pub fn timeline(
    exe: &Execution,
    steps: &[StepRun],
    records: Vec<LogRecord>,
    full: bool,
) -> Vec<LogEntry> {
    let mut b = Builder {
        lines: Vec::new(),
        full,
    };
    run_start(&mut b, exe);
    for run in steps {
        step_lines(&mut b, run);
    }
    for r in records {
        let group = steps
            .iter()
            .find(|s| {
                Some(&s.step_key) == r.step_key.as_ref()
                    && Some(s.attempt) == r.attempt
                    && s.kind == "action"
            })
            .map_or(i64::from(u32::MAX), |s| i64::from(s.seq));
        let sub = if r.kind == LogKind::Retry { 3 } else { 1 };
        let details = match (&r.details, full, r.kind) {
            (Some(d), false, LogKind::Http) => Some(viewer_details(d)),
            (_, false, _) => None,
            (d, true, _) => d.clone(),
        };
        let message = if full || r.kind != LogKind::Http {
            r.message
        } else {
            viewer_http_message(details.as_ref())
        };
        b.push(
            (r.at, group, sub),
            LogEntry {
                at: r.at,
                level: r.level,
                kind: r.kind,
                step_key: r.step_key,
                attempt: r.attempt,
                message,
                details,
            },
        );
    }
    run_end(&mut b, exe);
    b.lines.sort_by_key(|l| l.0);
    b.lines.into_iter().map(|(_, e)| e).collect()
}

fn viewer_http_message(details: Option<&Value>) -> String {
    let get = |k: &str| {
        details
            .and_then(|d| d.get(k))
            .cloned()
            .unwrap_or(Value::Null)
    };
    let status = get("status");
    let outcome = status
        .as_u64()
        .map_or_else(|| "no answer".to_owned(), |s| s.to_string());
    format!(
        "{} {} → {outcome} in {} ms",
        get("method").as_str().unwrap_or("?"),
        get("url").as_str().unwrap_or(""),
        get("duration_ms").as_i64().unwrap_or(0)
    )
}

fn run_start(b: &mut Builder, exe: &Execution) {
    let mode = serde_json::to_value(exe.mode)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    let mut queued = entry(
        exe.queued_at,
        LogLevel::Info,
        LogKind::Run,
        format!("Run queued ({mode})"),
    );
    if exe.rerun_of.is_some() {
        queued.details = Some(json!({"rerun_of": exe.rerun_of}));
    }
    b.push((exe.queued_at, i64::MIN, 0), queued);
    if let Some(at) = exe.started_at {
        b.push(
            (at, i64::MIN, 1),
            entry(at, LogLevel::Info, LogKind::Run, "Run started".into()),
        );
    }
}

fn run_end(b: &mut Builder, exe: &Execution) {
    let last = b
        .lines
        .iter()
        .map(|(k, _)| k.0)
        .max()
        .unwrap_or(exe.queued_at);
    let (at, level, message) = match exe.status {
        ExecutionStatus::Queued | ExecutionStatus::Running => return,
        ExecutionStatus::Waiting => (last, LogLevel::Info, "Waiting".to_owned()),
        ExecutionStatus::Succeeded => (
            exe.finished_at.unwrap_or(last),
            LogLevel::Info,
            format!("Run succeeded{}", took(exe)),
        ),
        ExecutionStatus::Failed => {
            let code = exe.error_code.as_deref().unwrap_or("error");
            let message = match (&exe.error_message, b.full) {
                (Some(m), true) => format!("Run failed: {code}: {m}"),
                _ => format!("Run failed: {code}"),
            };
            (exe.finished_at.unwrap_or(last), LogLevel::Error, message)
        }
        ExecutionStatus::Cancelled => (
            exe.finished_at.unwrap_or(last),
            LogLevel::Warn,
            "Run cancelled".to_owned(),
        ),
    };
    let mut end = entry(at, level, LogKind::Run, message);
    end.step_key.clone_from(&exe.failed_step_key);
    let mut details = serde_json::Map::new();
    if let Some(until) = exe
        .waiting_until
        .filter(|_| exe.status == ExecutionStatus::Waiting)
    {
        details.insert("until".into(), json!(until));
    }
    if exe.possible_duplicate {
        details.insert("possible_duplicate".into(), json!(true));
    }
    if !details.is_empty() {
        end.details = Some(Value::Object(details));
    }
    b.push((at, i64::MAX, 0), end);
}

fn took(exe: &Execution) -> String {
    match (exe.started_at, exe.finished_at) {
        (Some(s), Some(f)) => format!(" in {}", duration((f - s).num_milliseconds())),
        _ => String::new(),
    }
}

/// `182 ms`, `2.4 s`, `3 min 5 s`.
#[must_use]
pub fn duration(ms: i64) -> String {
    match ms {
        ..1_000 => format!("{} ms", ms.max(0)),
        1_000..60_000 => format!(
            "{:.1} s",
            f64::from(i32::try_from(ms).unwrap_or(i32::MAX)) / 1000.0
        ),
        _ => format!("{} min {} s", ms / 60_000, (ms % 60_000) / 1000),
    }
}

fn step_lines(b: &mut Builder, run: &StepRun) {
    let seq = i64::from(run.seq);
    let key = &run.step_key;
    let with = |mut e: LogEntry| {
        e.step_key = Some(key.clone());
        e.attempt = (run.kind == "action").then_some(run.attempt);
        e
    };
    if run.kind == "action" {
        let msg = format!("Step `{key}` started (attempt {})", run.attempt);
        b.push(
            (run.started_at, seq, 0),
            with(entry(run.started_at, LogLevel::Info, LogKind::Step, msg)),
        );
    }
    let at = run.finished_at.unwrap_or(run.started_at);
    if let Some((level, message, details)) = step_outcome(run, b.full) {
        let mut e = with(entry(at, level, LogKind::Step, message));
        e.details = details;
        b.push((at, seq, 2), e);
    }
    if b.full {
        for w in &run.warnings {
            b.push(
                (at, seq, 2),
                with(entry(
                    at,
                    LogLevel::Warn,
                    LogKind::Step,
                    format!("Step `{key}`: {w}"),
                )),
            );
        }
    }
}

fn error_text(run: &StepRun, full: bool) -> String {
    let get = |k: &str| {
        run.error
            .as_ref()
            .and_then(|e| e.get(k))
            .and_then(Value::as_str)
    };
    let code = get("code").unwrap_or("error");
    match get("message").filter(|_| full) {
        Some(m) => format!("{code}: {m}"),
        None => code.to_owned(),
    }
}

fn step_outcome(run: &StepRun, full: bool) -> Option<(LogLevel, String, Option<Value>)> {
    let key = &run.step_key;
    let out = |k: &str| run.output.as_ref().and_then(|o| o.get(k)).cloned();
    let took = run
        .finished_at
        .map(|f| duration((f - run.started_at).num_milliseconds()))
        .unwrap_or_default();
    Some(match (run.kind.as_str(), run.status) {
        (_, StepStatus::Running) if run.kind == "delay" => (
            LogLevel::Info,
            format!("Delay `{key}` started"),
            out("until").map(|u| json!({"until": u})),
        ),
        (_, StepStatus::Running) => return None,
        ("decision", _) => {
            let branch = out("branch").and_then(|b| b.as_str().map(str::to_owned));
            let msg = branch.map_or_else(
                || format!("Decision `{key}` took the otherwise branch"),
                |b| format!("Decision `{key}` took branch `{b}`"),
            );
            (LogLevel::Info, msg, None)
        }
        ("set_variable", _) => {
            let names: Vec<String> = run
                .output
                .as_ref()
                .and_then(Value::as_object)
                .map(|o| o.keys().cloned().collect())
                .unwrap_or_default();
            let msg = if full && !names.is_empty() {
                format!("Variables set in `{key}`: {}", names.join(", "))
            } else {
                format!("Variables set in `{key}`")
            };
            (LogLevel::Info, msg, None)
        }
        ("delay", _) => (LogLevel::Info, format!("Delay `{key}` finished"), None),
        ("stop", _) => (
            LogLevel::Info,
            format!("Stopped by `{key}`"),
            out("status").map(|s| json!({"status": s})),
        ),
        (_, StepStatus::Succeeded) => (
            LogLevel::Info,
            format!("Step `{key}` succeeded in {took}"),
            None,
        ),
        (_, StepStatus::Failed) => (
            LogLevel::Error,
            format!("Step `{key}` failed: {}", error_text(run, full)),
            None,
        ),
        (_, StepStatus::RetryScheduled) => (
            LogLevel::Warn,
            format!(
                "Step `{key}` attempt {} failed: {}",
                run.attempt,
                error_text(run, full)
            ),
            None,
        ),
        (_, StepStatus::Skipped) => (
            LogLevel::Warn,
            format!(
                "Step `{key}` failed and the run continued: {}",
                error_text(run, full)
            ),
            None,
        ),
        (_, StepStatus::UnknownOutcome) => (
            LogLevel::Warn,
            format!("Step `{key}`: the worker stopped before saving the answer (it runs again)"),
            None,
        ),
    })
}

#[cfg(test)]
#[path = "log_tests.rs"]
mod tests;
