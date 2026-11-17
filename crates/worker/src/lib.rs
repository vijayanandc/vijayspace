//! weft-worker: background processing. `run` for production, `drain` for deterministic tests.

use std::{sync::Arc, time::Duration};

use chrono::Duration as ChronoDuration;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use weft_app::{
    Deps,
    ports::ClaimedJob,
    runner::{JobOutcome, infra_backoff, run_execution},
    triggers::{enqueue_due_polls, fire_due_schedules, renew_due_subscriptions, run_poll},
};
use weft_domain::ids::{ExecutionId, RegistrationId};

/// Queues this worker serves, highest priority first.
pub const QUEUES: [&str; 4] = ["executions_test", "executions", "triggers", "sync_bulk"];
const LEASE_SECS: i64 = 90;

/// Worker settings.
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    pub id: String,
    pub concurrency: usize,
}

/// One scheduler pass: reap expired leases, fire schedules, enqueue polls. Only database work,
/// so a slow app API can never delay schedules or lease recovery.
pub async fn tick(deps: &Deps) {
    match deps.store.reap(deps.clock.now()).await {
        Ok(dead) => {
            for payload in dead {
                if let Err(e) = mark_dead_execution(deps, &payload).await {
                    tracing::warn!(error = %e, "failing the execution of a dead job");
                }
            }
        }
        Err(e) => tracing::warn!(error = %e, "reap"),
    }
    if let Err(e) = fire_due_schedules(deps).await {
        tracing::warn!(error = %e, "schedules");
    }
    if let Err(e) = enqueue_due_polls(deps).await {
        tracing::warn!(error = %e, "polls");
    }
}

/// Renews or retries app subscriptions (network calls to providers), on its own loop.
pub async fn renew(deps: &Deps) {
    if let Err(e) = renew_due_subscriptions(deps).await {
        tracing::warn!(error = %e, "subscriptions");
    }
}

/// Processes one claimed job and settles it (complete, reschedule or fail).
pub async fn process(deps: &Deps, job: ClaimedJob) {
    let fut = handle(deps, &job);
    tokio::pin!(fut);
    let mut heartbeat = tokio::time::interval(Duration::from_secs(20));
    heartbeat.tick().await;
    let result = loop {
        tokio::select! {
            r = &mut fut => break r,
            _ = heartbeat.tick() => {
                let until = deps.clock.now() + ChronoDuration::seconds(LEASE_SECS);
                match deps.store.heartbeat(&job, until).await {
                    Ok(true) => {}
                    Ok(false) => tracing::warn!(job = %job.id, "lease lost; another worker may run this job"),
                    Err(e) => tracing::warn!(job = %job.id, error = %e, "heartbeat failed"),
                }
            }
        }
    };
    let now = deps.clock.now();
    let settled = match result {
        Ok(JobOutcome::Done) => deps.store.complete(&job).await,
        Ok(JobOutcome::RescheduleAt(at)) => {
            let at = reschedule_at(at, now, cancel_pending(deps, &job).await);
            deps.store.reschedule(&job, at, now).await
        }
        Err(e) => {
            tracing::warn!(job = %job.id, kind = %job.kind, error = %e, "job failed");
            let retry_at = now + infra_backoff(job.attempts);
            match deps.store.fail(&job, &e.to_string(), retry_at, now).await {
                Ok(Some(true)) => mark_dead_execution(deps, &job.payload).await.map(|()| true),
                Ok(Some(false)) => Ok(true),
                Ok(None) => Ok(false),
                Err(e) => Err(e),
            }
        }
    };
    match settled {
        Ok(true) => {}
        Ok(false) => {
            tracing::warn!(job = %job.id, "lease lost before settling; the new holder settles it");
        }
        Err(e) => tracing::error!(job = %job.id, error = %e, "could not settle job"),
    }
}

/// A lease holder unique to one claim (worker ID plus a fresh UUID): a job reaped from a stalled
/// task and reclaimed by another task of the same process must not be settled by the first.
fn claim_token(deps: &Deps, worker: &str) -> String {
    format!("{worker}:{}", deps.ids.uuid().simple())
}

/// A cancel that arrived while the slice ran must not wait for a long delay or retry backoff:
/// the job runs again now and the next slice cancels the execution.
fn reschedule_at(
    at: chrono::DateTime<chrono::Utc>,
    now: chrono::DateTime<chrono::Utc>,
    cancel_requested: bool,
) -> chrono::DateTime<chrono::Utc> {
    if cancel_requested { now } else { at }
}

async fn cancel_pending(deps: &Deps, job: &ClaimedJob) -> bool {
    let Some(id) = execution_id(job) else {
        return false;
    };
    matches!(
        deps.store.get_execution_system(id).await,
        Ok(Some(exe)) if exe.cancel_requested && !exe.status.is_terminal()
    )
}

fn execution_id(job: &ClaimedJob) -> Option<ExecutionId> {
    execution_id_of(&job.payload)
}

fn execution_id_of(payload: &serde_json::Value) -> Option<ExecutionId> {
    payload
        .get("execution_id")
        .and_then(serde_json::Value::as_str)
        .and_then(|s| s.parse::<ExecutionId>().ok())
}

async fn handle(deps: &Deps, job: &ClaimedJob) -> Result<JobOutcome, weft_app::AppError> {
    match job.kind.as_str() {
        "run_execution" => match execution_id(job) {
            Some(id) => run_execution(deps, id).await,
            None => Ok(JobOutcome::Done),
        },
        "poll_trigger" => {
            if let Some(id) = job
                .payload
                .get("registration_id")
                .and_then(serde_json::Value::as_str)
                .and_then(|s| s.parse::<RegistrationId>().ok())
            {
                // Not retried by the queue: the next interval polls again, and retries would
                // pile up behind a broken connection.
                if let Err(e) = run_poll(deps, id).await {
                    tracing::warn!(registration = %id, error = %e, "poll failed");
                }
            }
            Ok(JobOutcome::Done)
        }
        other => {
            tracing::warn!(kind = %other, "unknown job kind; dropping");
            Ok(JobOutcome::Done)
        }
    }
}

/// A job that keeps failing for infrastructure reasons fails its execution so it is visible.
async fn mark_dead_execution(
    deps: &Deps,
    payload: &serde_json::Value,
) -> Result<(), weft_app::AppError> {
    let Some(id) = execution_id_of(payload) else {
        return Ok(());
    };
    let Some(exe) = deps.store.get_execution_system(id).await? else {
        return Ok(());
    };
    deps.store
        .finish_execution(&weft_app::ports::Finish {
            execution_id: id,
            status: weft_domain::model::ExecutionStatus::Failed,
            run_state: exe.run_state,
            error_code: Some("internal.poison_job".into()),
            error_message: Some("The execution could not be processed after many attempts".into()),
            failed_step_key: None,
            now: deps.clock.now(),
        })
        .await
}

/// Processes everything due *now* (per the injected clock) until nothing is left. For tests.
pub async fn drain(deps: &Deps) -> usize {
    let mut processed = 0;
    for _ in 0..1_000 {
        tick(deps).await;
        renew(deps).await;
        let now = deps.clock.now();
        let jobs = deps
            .store
            .claim(
                &QUEUES,
                &claim_token(deps, "drain"),
                50,
                now,
                now + ChronoDuration::seconds(LEASE_SECS),
            )
            .await
            .unwrap_or_default();
        if jobs.is_empty() {
            break;
        }
        for job in jobs {
            process(deps, job).await;
            processed += 1;
        }
    }
    processed
}

/// Runs until `shutdown` is cancelled: a scheduler loop plus a claim loop with bounded concurrency.
pub async fn run(deps: Deps, config: WorkerConfig, shutdown: CancellationToken) {
    let sched = {
        let deps = deps.clone();
        let shutdown = shutdown.clone();
        tokio::spawn(async move {
            let mut every = tokio::time::interval(Duration::from_secs(1));
            loop {
                tokio::select! {
                    () = shutdown.cancelled() => break,
                    _ = every.tick() => tick(&deps).await,
                }
            }
        })
    };
    let renewals = {
        let deps = deps.clone();
        let shutdown = shutdown.clone();
        tokio::spawn(async move {
            let mut every = tokio::time::interval(Duration::from_secs(30));
            loop {
                tokio::select! {
                    () = shutdown.cancelled() => break,
                    _ = every.tick() => renew(&deps).await,
                }
            }
        })
    };
    let permits = Arc::new(Semaphore::new(config.concurrency.max(1)));
    let mut idle = Duration::from_millis(200);
    while !shutdown.is_cancelled() {
        let free = permits.available_permits();
        let now = deps.clock.now();
        let jobs = if free == 0 {
            Vec::new()
        } else {
            deps.store
                .claim(
                    &QUEUES,
                    &claim_token(&deps, &config.id),
                    u32::try_from(free).unwrap_or(1),
                    now,
                    now + ChronoDuration::seconds(LEASE_SECS),
                )
                .await
                .unwrap_or_else(|e| {
                    tracing::warn!(error = %e, "claim failed");
                    Vec::new()
                })
        };
        if jobs.is_empty() {
            tokio::select! {
                () = shutdown.cancelled() => break,
                () = deps.wake.notified() => {},
                () = tokio::time::sleep(idle) => {},
            }
            idle = (idle * 2).min(Duration::from_secs(1));
            continue;
        }
        idle = Duration::from_millis(200);
        for job in jobs {
            let Ok(permit) = permits.clone().acquire_owned().await else {
                break;
            };
            let deps = deps.clone();
            tokio::spawn(async move {
                process(&deps, job).await;
                drop(permit);
            });
        }
    }
    // Graceful shutdown: wait for in-flight jobs (bounded); unfinished leases expire and are reaped.
    let _ = tokio::time::timeout(
        Duration::from_secs(30),
        permits.acquire_many(u32::try_from(config.concurrency.max(1)).unwrap_or(1)),
    )
    .await;
    sched.abort();
    renewals.abort();
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone as _, Utc};

    use super::reschedule_at;

    #[test]
    fn a_pending_cancel_runs_the_job_now_instead_of_after_the_delay() {
        let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let in_a_week = now + Duration::days(7);
        assert_eq!(reschedule_at(in_a_week, now, true), now);
        assert_eq!(reschedule_at(in_a_week, now, false), in_a_week);
    }
}
