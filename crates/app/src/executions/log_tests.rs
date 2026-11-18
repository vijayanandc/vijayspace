//! The run log as one chronological list, and what viewers see of it.

use chrono::{Duration, TimeZone as _};
use uuid::Uuid;
use weft_domain::{
    ids::{FlowId, FlowVersionId, OrgId, StepRunId},
    model::ExecutionMode,
};

use super::*;

fn t(s: i64) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 18, 9, 0, 0).unwrap() + Duration::milliseconds(s)
}

fn exe(status: ExecutionStatus) -> Execution {
    Execution {
        id: ExecutionId::from_uuid(Uuid::nil()),
        org_id: OrgId::from_uuid(Uuid::nil()),
        flow_id: FlowId::from_uuid(Uuid::nil()),
        flow_version_id: FlowVersionId::from_uuid(Uuid::nil()),
        trigger_event_id: None,
        mode: ExecutionMode::Live,
        rerun_of: None,
        status,
        trigger_data: Value::Null,
        run_state: Value::Null,
        waiting_until: None,
        task_count: 1,
        error_code: Some("app.unavailable".into()),
        error_message: Some("the app said: account 42 is locked".into()),
        failed_step_key: Some("crm".into()),
        possible_duplicate: false,
        cancel_requested: false,
        queued_at: t(0),
        started_at: Some(t(10)),
        finished_at: Some(t(5000)),
    }
}

fn step(seq: u32, attempt: u16, status: StepStatus, from: i64, to: i64) -> StepRun {
    StepRun {
        id: StepRunId::from_uuid(Uuid::nil()),
        execution_id: ExecutionId::from_uuid(Uuid::nil()),
        seq,
        step_key: "crm".into(),
        kind: "action".into(),
        attempt,
        status,
        input: None,
        output: None,
        error: (status != StepStatus::Succeeded).then(
            || json!({"code": "app.unavailable", "message": "the app said: account 42 is locked"}),
        ),
        warnings: Vec::new(),
        started_at: t(from),
        finished_at: Some(t(to)),
    }
}

fn http(attempt: u16, status: u16, at: i64) -> LogRecord {
    LogRecord {
        id: Uuid::nil(),
        execution_id: ExecutionId::from_uuid(Uuid::nil()),
        step_key: Some("crm".into()),
        attempt: Some(attempt),
        level: if status >= 500 {
            LogLevel::Warn
        } else {
            LogLevel::Info
        },
        kind: LogKind::Http,
        message: format!("POST https://crm.example.com/v2/Leads/42 → {status} in 120 ms"),
        details: Some(
            json!({"method": "POST", "url": "https://crm.example.com/v2/Leads/42", "status": status,
            "duration_ms": 120, "response_excerpt": "account 42 is locked"}),
        ),
        at: t(at),
    }
}

fn retry(at: i64) -> LogRecord {
    LogRecord {
        kind: LogKind::Retry,
        level: LogLevel::Warn,
        message: "Attempt 1 failed (app.unavailable); attempt 2 is scheduled".into(),
        details: Some(json!({"retry_at": "2026-11-18T09:01:00Z"})),
        at: t(at),
        ..http(1, 0, 0)
    }
}

fn messages(entries: &[LogEntry]) -> Vec<&str> {
    entries.iter().map(|e| e.message.as_str()).collect()
}

#[test]
fn a_run_log_reads_in_order_with_each_attempt_its_calls_and_its_retry() {
    let steps = [
        step(0, 1, StepStatus::RetryScheduled, 20, 200),
        step(1, 2, StepStatus::Failed, 3000, 3300),
    ];
    // Stored out of order: the log sorts by time, then by step run.
    let records = vec![http(2, 503, 3100), retry(200), http(1, 503, 50)];
    let log = timeline(&exe(ExecutionStatus::Failed), &steps, records, true);
    assert_eq!(
        messages(&log),
        [
            "Run queued (live)",
            "Run started",
            "Step `crm` started (attempt 1)",
            "POST https://crm.example.com/v2/Leads/42 → 503 in 120 ms",
            "Step `crm` attempt 1 failed: app.unavailable: the app said: account 42 is locked",
            "Attempt 1 failed (app.unavailable); attempt 2 is scheduled",
            "Step `crm` started (attempt 2)",
            "POST https://crm.example.com/v2/Leads/42 → 503 in 120 ms",
            "Step `crm` failed: app.unavailable: the app said: account 42 is locked",
            "Run failed: app.unavailable: the app said: account 42 is locked",
        ]
    );
    assert_eq!(log[3].attempt, Some(1));
    assert_eq!(log[7].attempt, Some(2));
    assert_eq!(log[9].level, LogLevel::Error);
    assert_eq!(log[9].step_key.as_deref(), Some("crm"));
}

#[test]
fn viewers_see_codes_timings_and_the_apps_address_but_no_data() {
    let steps = [step(0, 1, StepStatus::Failed, 20, 200)];
    let log = timeline(
        &exe(ExecutionStatus::Failed),
        &steps,
        vec![http(1, 503, 50)],
        false,
    );
    let text = serde_json::to_string(&log).unwrap();
    assert!(!text.contains("locked"), "no app messages: {text}");
    assert!(!text.contains("Leads/42"), "no record paths: {text}");
    assert_eq!(
        log[3].message,
        "POST https://crm.example.com → 503 in 120 ms"
    );
    assert_eq!(
        log[3].details,
        Some(
            json!({"method": "POST", "url": "https://crm.example.com", "status": 503, "duration_ms": 120})
        )
    );
    assert_eq!(log[4].message, "Step `crm` failed: app.unavailable");
    assert_eq!(log[5].message, "Run failed: app.unavailable");
}

#[test]
fn a_waiting_run_says_until_when_and_durations_read_naturally() {
    let mut waiting = exe(ExecutionStatus::Waiting);
    waiting.finished_at = None;
    waiting.waiting_until = Some(t(60_000));
    let log = timeline(&waiting, &[], Vec::new(), true);
    let last = log.last().unwrap();
    assert_eq!(last.message, "Waiting");
    assert_eq!(last.details, Some(json!({"until": t(60_000)})));
    assert_eq!(duration(182), "182 ms");
    assert_eq!(duration(2_400), "2.4 s");
    assert_eq!(duration(185_000), "3 min 5 s");
}

#[test]
fn a_failure_the_run_continued_past_is_a_warning_not_the_runs_failure() {
    let mut run = exe(ExecutionStatus::Succeeded);
    run.failed_step_key = None;
    let failed = step(0, 1, StepStatus::Failed, 20, 40);
    let log = timeline(&run, std::slice::from_ref(&failed), Vec::new(), true);
    let line = log
        .iter()
        .find(|e| e.message.contains("continued"))
        .unwrap();
    assert_eq!(line.level, LogLevel::Warn);
    assert_eq!(
        line.message,
        "Step `crm` failed and the run continued: app.unavailable: the app said: account 42 is locked"
    );
    // The step that failed the run, or one retried later by hand, stays an error.
    let log = timeline(
        &exe(ExecutionStatus::Failed),
        std::slice::from_ref(&failed),
        Vec::new(),
        true,
    );
    assert!(
        log.iter()
            .any(|e| e.message.starts_with("Step `crm` failed: ") && e.level == LogLevel::Error)
    );
    let retried = [failed, step(1, 2, StepStatus::Succeeded, 50, 60)];
    let log = timeline(&run, &retried, Vec::new(), true);
    assert!(
        log.iter()
            .any(|e| e.message.starts_with("Step `crm` failed: "))
    );
}
