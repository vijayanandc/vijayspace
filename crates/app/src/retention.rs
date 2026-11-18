//! Run history retention (doc 05 §4). History is kept at most 30 days: run history lives in day partitions,
//! and partitions past the cap are dropped (their space goes back to the operating system). An org that keeps
//! less has its older finished runs deleted in small batches; platform housekeeping removes dead jobs, expired
//! OAuth states and leases, old counters, dedupe keys and unused test snapshots.

use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use weft_domain::{error_codes, model::Org};

use crate::tenancy::Actor;
use crate::{
    AppError, Deps,
    ports::{PartitionReport, Purged},
};
use weft_domain::permissions::Permission;

/// Rows per batch (executions, events or snapshots; step runs go in smaller chunks).
const BATCH: u32 = 200;
/// Batches per org per sweep, so one sweep never hogs the database; the next sweep (every 15
/// minutes) continues. 200 × 200 per 15 minutes keeps up with ~40 finished runs per second.
const MAX_BATCHES: u32 = 200;
/// Allowed retention, in days. `MAX_DAYS` is the platform cap for everything kept about runs.
pub const MIN_DAYS: u32 = 1;
pub const MAX_DAYS: u32 = 30;
/// A run must finish, and can be retried, within this many days of being queued. A run's history goes with the
/// day it was queued (dropped 30 to 31 days on); a run still unfinished a day before that is failed with
/// `execution.expired` (`crates/db/src/partitions.rs`, `EXPIRE_AHEAD_DAYS`). This leaves a day's margin.
pub const RUN_DAYS: u32 = MAX_DAYS - 2;
/// Daily connection usage and send counters are kept this long (within the cap).
const USAGE_DAYS: i64 = 30;
/// Day partitions are created this many days ahead.
const AHEAD_DAYS: u32 = 7;

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
    pub invitations: u64,
    pub dedupe_keys: u64,
    pub partitions: PartitionReport,
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

/// Runs that finished before this time are (or are about to be) deleted by the sweep.
pub fn history_cutoff(deps: &Deps, org: &Org, now: DateTime<Utc>) -> DateTime<Utc> {
    now - Duration::days(i64::from(effective_days(deps, org)))
}

/// The sweep's lease key (32 bytes, as sync leases), held under the default org.
const SWEEP_LEASE: &[u8; 32] = b"retention-sweep\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";

/// One sweep over every org, then platform housekeeping. One worker sweeps at a time (a lease): several would
/// delete the same rows and wait on each other's locks.
pub async fn sweep(deps: &Deps) -> Result<SweepReport, AppError> {
    let now = deps.clock.now();
    let holder = format!("sweep-{}", deps.ids.uuid().simple());
    let lease_org = deps.config.default_org_id;
    let leased = deps
        .store
        .acquire_lease(
            lease_org,
            SWEEP_LEASE,
            &holder,
            now + Duration::minutes(20),
            now,
        )
        .await?;
    if !leased {
        return Ok(SweepReport::default());
    }
    let report = sweep_all(deps, now).await;
    let _ = deps
        .store
        .release_lease(lease_org, SWEEP_LEASE, &holder)
        .await;
    report
}

async fn sweep_all(deps: &Deps, now: DateTime<Utc>) -> Result<SweepReport, AppError> {
    // A failed partition step (e.g. a lock held by a long transaction) must not stop the per-org purges and
    // housekeeping: it is logged and the next sweep tries again.
    let partitions = match deps
        .store
        .maintain_partitions(now.date_naive(), MAX_DAYS, AHEAD_DAYS, now)
        .await
    {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, "partition maintenance failed; the next sweep tries again");
            PartitionReport::default()
        }
    };
    let mut report = SweepReport {
        partitions,
        ..SweepReport::default()
    };
    if report.partitions.rows_in_catch_all > 0 {
        tracing::warn!(
            rows = report.partitions.rows_in_catch_all,
            "run history rows sit in the catch-all partition (more than a week ahead, or maintenance lapsed)"
        );
    }
    for org in deps.store.list_orgs().await? {
        // Orgs that keep the full 30 days lose their history with the day partitions.
        if effective_days(deps, &org) >= MAX_DAYS {
            continue;
        }
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
    let history_before = now - Duration::days(i64::from(MAX_DAYS));
    // Batches until a round comes back short (bounded), so a busy day does not leave a growing backlog.
    for _ in 0..MAX_BATCHES {
        let h = deps
            .store
            .housekeeping(jobs_before, now, usage_before, history_before, BATCH)
            .await?;
        report.dead_jobs += h.dead_jobs;
        report.oauth_states += h.oauth_states;
        report.sync_leases += h.sync_leases;
        report.usage_days += h.usage_days;
        report.invitations += h.invitations;
        report.dedupe_keys += h.dedupe_keys;
        report.test_snapshots += h.test_snapshots;
        let most = [
            h.dedupe_keys,
            h.test_snapshots,
            h.dead_jobs,
            h.oauth_states,
            h.sync_leases,
            h.usage_days,
            h.invitations,
        ]
        .into_iter()
        .max()
        .unwrap_or(0);
        if most < u64::from(BATCH) {
            break;
        }
    }
    Ok(report)
}

async fn sweep_org(
    deps: &Deps,
    org: &Org,
    now: DateTime<Utc>,
    report: &mut SweepReport,
) -> Result<(), AppError> {
    let before = history_cutoff(deps, org, now);
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
    actor: &Actor,
    days: Option<u32>,
) -> Result<Org, AppError> {
    actor.require(Permission::OrgManage)?;
    let org = actor.org;
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
