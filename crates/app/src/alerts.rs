//! Staff alerts, like the Appblocks tenant monitor. Every 5 minutes one server (a lease) looks for:
//! - runs waiting too long in the queue;
//! - jobs that stopped retrying;
//! - orgs with many failed runs;
//! - connections that need signing in again;
//! - no worker running at all.
//!
//! New conditions are logged and emailed to `WEFT_ALERT_EMAILS`; one condition is not repeated within an hour.
//! Staff see the current conditions in `GET /api/v1/ops/status`.

use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use weft_connector_sdk::EmailMessage;

use crate::{
    AppError, Deps,
    ports::{AlertFacts, QueueStats},
};

/// How often the check runs.
pub const CHECK_SECS: u64 = 300;
/// A queue whose oldest due job waited longer than this is lagging.
const LAG_SECS: i64 = 300;
/// Failed runs of one org within the window that raise an alert.
const FAILED_RUNS: u64 = 10;
/// The window for dead jobs and failed runs.
const WINDOW_MINS: i64 = 60;
/// One condition is emailed at most once per this many minutes.
const REPEAT_MINS: i64 = 60;
const CHECK_LEASE: &[u8; 32] = b"alerts-check\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";

/// One condition staff should look at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Alert {
    /// Stable per condition (e.g. `lag:executions`, `reauth:<connection>`): repeats are recognised by it.
    pub key: String,
    /// `critical` or `warning`.
    pub severity: &'static str,
    pub title: String,
    pub detail: String,
}

fn alert(key: String, severity: &'static str, title: String, detail: &str) -> Alert {
    Alert {
        key,
        severity,
        title,
        detail: detail.to_owned(),
    }
}

/// The alerts these facts raise (pure).
#[must_use]
pub fn evaluate(
    now: DateTime<Utc>,
    queues: &QueueStats,
    facts: &AlertFacts,
    live_workers: usize,
) -> Vec<Alert> {
    let mut out = Vec::new();
    if live_workers == 0 {
        out.push(alert(
            "no-worker".into(),
            "critical",
            "No worker is running".into(),
            "Runs, schedules and polls wait until a `weft worker` (or `weft serve`) starts.",
        ));
    }
    for q in &queues.queues {
        let waited = q.oldest_due.map_or(0, |t| (now - t).num_seconds());
        if waited > LAG_SECS {
            out.push(alert(
                format!("lag:{}", q.queue),
                "warning",
                format!("Jobs in `{}` have waited {} min ({} due)", q.queue, waited / 60, q.ready),
                "The workers are not keeping up: start another `weft worker`, or check the workers' logs.",
            ));
        }
    }
    if facts.new_dead_jobs > 0 {
        out.push(alert(
            "dead-jobs".into(),
            "warning",
            format!("{} jobs stopped retrying in the last hour", facts.new_dead_jobs),
            "Their worker kept failing (often the database or a crash). The runs failed; see the server logs.",
        ));
    }
    for org in facts.failed_runs.iter().filter(|o| o.failed >= FAILED_RUNS) {
        out.push(alert(
            format!("failed:{}", org.org_id),
            "warning",
            format!(
                "{} runs failed in {} in the last hour",
                org.failed, org.org_name
            ),
            "Open History in that organisation and filter by Failed to see why.",
        ));
    }
    for c in &facts.needs_reauth {
        out.push(alert(
            format!("reauth:{}", c.connection_id),
            "warning",
            format!(
                "Connection {} ({}) in {} needs signing in again",
                c.name, c.connector, c.org_name
            ),
            "Its runs fail until someone reconnects it under Connections.",
        ));
    }
    out
}

/// The conditions now (for staff's status page).
pub async fn current(deps: &Deps) -> Result<Vec<Alert>, AppError> {
    let now = deps.clock.now();
    let queues = deps.store.queue_stats(now).await?;
    let facts = deps
        .store
        .alert_facts(now - Duration::minutes(WINDOW_MINS))
        .await?;
    let workers = deps
        .store
        .live_instances(now - Duration::seconds(45))
        .await?
        .iter()
        .filter(|i| i.kind != "api")
        .count();
    Ok(evaluate(now, &queues, &facts, workers))
}

/// One check: logs and emails the conditions not reported within the last hour. Returns how many were new.
pub async fn check(deps: &Deps) -> Result<usize, AppError> {
    let now = deps.clock.now();
    let org = deps.config.default_org_id;
    let holder = format!("alerts-{}", deps.ids.uuid().simple());
    if !deps
        .store
        .acquire_lease(org, CHECK_LEASE, &holder, now + Duration::minutes(2), now)
        .await?
    {
        return Ok(0);
    }
    let mut fresh = Vec::new();
    for a in current(deps).await? {
        let key: [u8; 32] = Sha256::digest(format!("alert:{}", a.key).as_bytes()).into();
        // The lease is the "already reported" mark: it lapses after an hour, then the alert may be sent again.
        // Each check has its own holder, so another check never re-takes an unexpired mark.
        if deps
            .store
            .acquire_lease(
                org,
                &key,
                &holder,
                now + Duration::minutes(REPEAT_MINS),
                now,
            )
            .await?
        {
            tracing::warn!(alert = %a.key, severity = a.severity, "{}", a.title);
            fresh.push(a);
        }
    }
    let _ = deps.store.release_lease(org, CHECK_LEASE, &holder).await;
    if !fresh.is_empty() {
        email(deps, &fresh).await;
    }
    Ok(fresh.len())
}

async fn email(deps: &Deps, alerts: &[Alert]) {
    let (Some(mailer), false) = (&deps.mailer, deps.config.alert_emails.is_empty()) else {
        return;
    };
    let body = alerts
        .iter()
        .map(|a| format!("[{}] {}\n{}\n", a.severity, a.title, a.detail))
        .collect::<Vec<_>>()
        .join("\n");
    let message = EmailMessage {
        to: deps.config.alert_emails.clone(),
        cc: Vec::new(),
        subject: format!(
            "[Weft] {} alert{}: {}",
            alerts.len(),
            if alerts.len() == 1 { "" } else { "s" },
            alerts[0].title
        ),
        body: format!(
            "{body}\nSee the platform status: {}/settings/platform\n",
            deps.config.app_url
        ),
        html: false,
        reply_to: None,
    };
    if let Err(e) = mailer.send(&message).await {
        tracing::warn!(error = %e, "sending the alert email");
    }
}

#[cfg(test)]
#[path = "alerts_tests.rs"]
mod tests;
