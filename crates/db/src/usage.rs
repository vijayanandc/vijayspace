//! Monthly usage counters (tasks and executions per org), bumped inside execution transactions.

use chrono::{DateTime, Datelike as _, Utc};
use sqlx::{MySql, Transaction};
use weft_domain::ids::OrgId;

/// `YYYY-MM` of a moment (UTC), the usage period.
pub(crate) fn period(now: DateTime<Utc>) -> String {
    format!("{:04}-{:02}", now.year(), now.month())
}

/// Adds tasks and executions to the org's counter for the period of `now`.
pub(crate) async fn bump_usage(
    tx: &mut Transaction<'_, MySql>,
    org: OrgId,
    now: DateTime<Utc>,
    tasks: u32,
    executions: u32,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO usage_counters (org_id, period, tasks_used, executions, updated_at) VALUES (?, ?, ?, ?, ?) AS new \
         ON DUPLICATE KEY UPDATE tasks_used = usage_counters.tasks_used + new.tasks_used, executions = usage_counters.executions + new.executions, updated_at = new.updated_at",
    )
    .bind(org.as_uuid())
    .bind(period(now))
    .bind(tasks)
    .bind(executions)
    .bind(now)
    .execute(&mut **tx)
    .await
    .map(|_| ())
}
