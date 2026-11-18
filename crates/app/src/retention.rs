//! Run history retention (doc 05 §4): finished runs older than the org's retention are deleted
//! in small batches, with their step runs, trigger events and test snapshots; platform
//! housekeeping removes dead jobs, expired OAuth states and leases, and old usage counters.

use chrono::Duration;
use serde::Serialize;
use weft_domain::{error_codes, ids::OrgId, model::Org};

use crate::{AppError, Deps, ports::Purged};

/// Rows per batch (executions, events or snapshots; step runs go in smaller chunks).
const BATCH: u32 = 200;
/// Batches per org per sweep, so one sweep never hogs the database; the next sweep (every 15
/// minutes) continues. 200 × 200 per 15 minutes keeps up with ~40 finished runs per second.
const MAX_BATCHES: u32 = 200;
/// Allowed retention, in days.
pub const MIN_DAYS: u32 = 1;
pub const MAX_DAYS: u32 = 3_650;
/// Daily connection usage counters are kept this long.
const USAGE_DAYS: i64 = 90;

/// What one sweep removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct SweepReport {
    pub executions: u64,
    pub step_runs: u64,
    pub trigger_events: u64,
    pub test_snapshots: u64,
    pub dead_jobs: u64,
    pub oauth_states: u64,
    pub sync_leases: u64,
    pub usage_days: u64,
}

impl SweepReport {
    fn add(&mut self, p: Purged) {
        self.executions += p.executions;
        self.step_runs += p.step_runs;
        self.trigger_events += p.trigger_events;
        self.test_snapshots += p.test_snapshots;
    }
}

/// The org's retention in days (its own setting, else the server default).
#[must_use]
pub fn effective_days(deps: &Deps, org: &Org) -> u32 {
    org.retention_days
        .unwrap_or(deps.config.default_retention_days)
        .clamp(MIN_DAYS, MAX_DAYS)
}

/// One sweep over every org, then platform housekeeping.
pub async fn sweep(deps: &Deps) -> Result<SweepReport, AppError> {
    let now = deps.clock.now();
    let mut report = SweepReport::default();
    for org in deps.store.list_orgs().await? {
        // One org's failure (e.g. a lock wait timeout) must not stop the others.
        if let Err(e) = sweep_org(deps, &org, now, &mut report).await {
            tracing::warn!(org = %org.id, error = %e, "retention sweep of an org failed");
        }
    }
    let jobs_before = now
        - Duration::days(i64::from(
            deps.config.default_retention_days.clamp(MIN_DAYS, MAX_DAYS),
        ));
    let usage_before = (now - Duration::days(USAGE_DAYS)).date_naive();
    let h = deps
        .store
        .housekeeping(jobs_before, now, usage_before, BATCH)
        .await?;
    report.dead_jobs = h.dead_jobs;
    report.oauth_states = h.oauth_states;
    report.sync_leases = h.sync_leases;
    report.usage_days = h.usage_days;
    Ok(report)
}

async fn sweep_org(
    deps: &Deps,
    org: &Org,
    now: chrono::DateTime<chrono::Utc>,
    report: &mut SweepReport,
) -> Result<(), AppError> {
    let before = now - Duration::days(i64::from(effective_days(deps, org)));
    for _ in 0..MAX_BATCHES {
        let purged = deps.store.purge_history(org.id, before, BATCH).await?;
        report.add(purged);
        let full = [
            purged.executions,
            purged.trigger_events,
            purged.test_snapshots,
        ]
        .into_iter()
        .any(|n| n >= u64::from(BATCH));
        if !full {
            break;
        }
    }
    Ok(())
}

/// Sets the org's retention (`None` = the server default).
pub async fn set_retention_days(
    deps: &Deps,
    org: OrgId,
    days: Option<u32>,
) -> Result<Org, AppError> {
    if let Some(d) = days
        && !(MIN_DAYS..=MAX_DAYS).contains(&d)
    {
        return Err(AppError::validation(
            error_codes::VALIDATION_FAILED,
            format!("Retention must be between {MIN_DAYS} and {MAX_DAYS} days"),
        ));
    }
    deps.store
        .set_retention_days(org, days, deps.clock.now())
        .await?;
    deps.store
        .get_org(org)
        .await?
        .ok_or_else(|| AppError::not_found(error_codes::NOT_FOUND, "Org not found"))
}
