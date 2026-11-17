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

/// One scheduler pass: fire schedules, enqueue polls, renew subscriptions, reap leases.
pub async fn tick(deps: &Deps) {
    if let Err(e) = fire_due_schedules(deps).await {
        tracing::warn!(error = %e, "schedules");
    }
    if let Err(e) = enqueue_due_polls(deps).await {
        tracing::warn!(error = %e, "polls");
    }
    if let Err(e) = renew_due_subscriptions(deps).await {
        tracing::warn!(error = %e, "subscriptions");
    }
    if let Err(e) = deps.store.reap(deps.clock.now()).await {
        tracing::warn!(error = %e, "reap");
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
                let _ = deps.store.heartbeat(job.id, deps.clock.now() + ChronoDuration::seconds(LEASE_SECS)).await;
            }
        }
    };
    let now = deps.clock.now();
    let settled = match result {
        Ok(JobOutcome::Done) => deps.store.complete(job.id).await,
        Ok(JobOutcome::RescheduleAt(at)) => deps.store.reschedule(job.id, at, now).await,
        Err(e) => {
            tracing::warn!(job = %job.id, kind = %job.kind, error = %e, "job failed");
            match deps
                .store
                .fail(
                    job.id,
                    &e.to_string(),
                    now + infra_backoff(job.attempts),
                    now,
                )
                .await
            {
                Ok(true) => mark_dead_execution(deps, &job).await,
                Ok(false) => Ok(()),
                Err(e) => Err(e),
            }
        }
    };
    if let Err(e) = settled {
        tracing::error!(job = %job.id, error = %e, "could not settle job");
    }
}

async fn handle(deps: &Deps, job: &ClaimedJob) -> Result<JobOutcome, weft_app::AppError> {
    match job.kind.as_str() {
        "run_execution" => match job
            .payload
            .get("execution_id")
            .and_then(serde_json::Value::as_str)
            .and_then(|s| s.parse::<ExecutionId>().ok())
        {
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
                run_poll(deps, id).await?;
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
async fn mark_dead_execution(deps: &Deps, job: &ClaimedJob) -> Result<(), weft_app::AppError> {
    let Some(id) = job
        .payload
        .get("execution_id")
        .and_then(serde_json::Value::as_str)
        .and_then(|s| s.parse::<ExecutionId>().ok())
    else {
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
        let now = deps.clock.now();
        let jobs = deps
            .store
            .claim(
                &QUEUES,
                "drain",
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
                    &config.id,
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
}
