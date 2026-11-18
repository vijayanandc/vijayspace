//! Day partitions of run history (`executions`, `step_runs`, `trigger_events`; migration 20261119000004), kept
//! the way the Appblocks servers keep `request_log`: day partitions are created a week ahead by reorganising
//! the `pmax` catch-all (rows already in it move to their day), and partitions older than the history cap are
//! dropped, which returns their space to the operating system. `pmax` is never dropped (excluded by name).
//!
//! Partition DDL cannot bind parameters: names and bounds are built from dates and a fixed table list only.
//! Several servers may run maintenance at once (the sweep lease can lapse): "already exists" and "already gone"
//! errors are treated as done.

use chrono::{DateTime, Duration, NaiveDate, Utc};
use sqlx::QueryBuilder;
use uuid::Uuid;
use weft_app::ports::{PartitionReport, Res};
use weft_domain::ids::ExecutionId;

use crate::{MySqlStore, codec::db_err};

/// The partitioned tables (a run's partition holds its steps; trigger events have their own days).
const TABLES: [&str; 3] = ["executions", "step_runs", "trigger_events"];
/// Rows that landed in `pmax` (maintenance lapsed): counted per table for the report.
const PMAX_ROWS: [&str; 3] = [
    "SELECT COUNT(*) FROM executions PARTITION (pmax)",
    "SELECT COUNT(*) FROM step_runs PARTITION (pmax)",
    "SELECT COUNT(*) FROM trigger_events PARTITION (pmax)",
];
/// The catch-all partition: never dropped.
const CATCH_ALL: &str = "pmax";
/// Day partitions created in one statement at most (normal operation needs one a day).
const MAX_NEW_PER_TABLE: i64 = 64;
/// Unfinished runs expired per sweep before their partition is dropped.
const EXPIRE_BATCH: i64 = 1000;
/// MySQL: duplicate partition name (another server created it) / dropping a partition that is gone.
const BENIGN: [&str; 2] = ["1517", "1507"];

/// `pYYYYMMDD` holds that day.
#[must_use]
pub fn partition_name(day: NaiveDate) -> String {
    format!("p{}", day.format("%Y%m%d"))
}

/// One partition: its name and exclusive upper bound (`None` for `pmax`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Partition {
    pub name: String,
    pub below: Option<NaiveDate>,
}

/// The `REORGANIZE` that splits `pmax` into day partitions from `from` through `to`, or `None` when there is
/// nothing to add. `first_catches_older`: on a table with no day partitions yet, the first one also takes every
/// older row (rows queued before the table was partitioned), so they become droppable.
#[must_use]
pub fn reorganize_ddl(table: &str, from: NaiveDate, to: NaiveDate) -> Option<String> {
    if from > to {
        return None;
    }
    let mut parts = Vec::new();
    let mut day = from;
    while day <= to && parts.len() < usize::try_from(MAX_NEW_PER_TABLE).unwrap_or(64) {
        let next = day + Duration::days(1);
        parts.push(format!(
            "PARTITION {} VALUES LESS THAN ('{}')",
            partition_name(day),
            next.format("%Y-%m-%d")
        ));
        day = next;
    }
    parts.push(format!("PARTITION {CATCH_ALL} VALUES LESS THAN (MAXVALUE)"));
    Some(format!(
        "ALTER TABLE {table} REORGANIZE PARTITION {CATCH_ALL} INTO ({})",
        parts.join(", ")
    ))
}

/// The partitions whose rows are all older than `keep_from` (their bound is at or before it). Never `pmax`.
#[must_use]
pub fn droppable(parts: &[Partition], keep_from: NaiveDate) -> Vec<String> {
    parts
        .iter()
        .filter(|p| p.name != CATCH_ALL && p.below.is_some_and(|b| b <= keep_from))
        .map(|p| p.name.clone())
        .collect()
}

/// Reads `'2026-11-19'` (RANGE COLUMNS bounds are quoted) or `MAXVALUE`.
fn bound(description: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(description.trim().trim_matches('\''), "%Y-%m-%d").ok()
}

fn benign(e: &sqlx::Error) -> bool {
    e.as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
        .is_some_and(|c| BENIGN.contains(&c.as_ref()))
}

impl MySqlStore {
    async fn partitions(&self, table: &str) -> Res<Vec<Partition>> {
        let rows: Vec<(String, Option<String>)> = sqlx::query_as(
            // information_schema columns come back as BLOBs: read them as text.
            "SELECT CAST(partition_name AS CHAR), CAST(partition_description AS CHAR) FROM information_schema.partitions \
             WHERE table_schema = DATABASE() AND table_name = ? AND partition_name IS NOT NULL \
             ORDER BY partition_ordinal_position",
        )
        .bind(table)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(rows
            .into_iter()
            .map(|(name, description)| Partition {
                below: description.as_deref().and_then(bound),
                name,
            })
            .collect())
    }

    async fn ddl(&self, statement: &str) -> Res<bool> {
        match sqlx::query(statement).execute(&self.pool).await {
            Ok(_) => Ok(true),
            Err(e) if benign(&e) => Ok(false),
            Err(e) => Err(db_err(e)),
        }
    }

    /// Fails runs that are still unfinished in partitions about to be dropped (they outlived the history cap)
    /// and removes their jobs, so nothing points at a run that no longer exists.
    async fn expire_unfinished(&self, keep_from: NaiveDate, now: DateTime<Utc>) -> Res<u64> {
        let ids: Vec<Uuid> = sqlx::query_scalar(
            "SELECT id FROM executions WHERE day < ? AND status NOT IN ('succeeded', 'failed', 'cancelled') LIMIT ?",
        )
        .bind(keep_from)
        .bind(EXPIRE_BATCH)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;
        if ids.is_empty() {
            return Ok(0);
        }
        let mut tx = self.pool.begin().await.map_err(db_err)?;
        let mut update = QueryBuilder::new(
            "UPDATE executions SET status = 'failed', error_code = 'execution.expired', \
             error_message = 'The run passed the 30-day history limit before it finished', finished_at = ",
        );
        update.push_bind(now).push(", updated_at = ").push_bind(now);
        update
            .push(" WHERE day < ")
            .push_bind(keep_from)
            .push(" AND id IN (");
        let mut list = update.separated(", ");
        for id in &ids {
            list.push_bind(*id);
        }
        update.push(")");
        update.build().execute(&mut *tx).await.map_err(db_err)?;
        let mut jobs = QueryBuilder::new("DELETE FROM jobs WHERE dedupe_key IN (");
        let mut keys = jobs.separated(", ");
        for id in &ids {
            keys.push_bind(format!("exe:{}", ExecutionId::from_uuid(*id)));
        }
        jobs.push(")");
        jobs.build().execute(&mut *tx).await.map_err(db_err)?;
        tx.commit().await.map_err(db_err)?;
        Ok(u64::try_from(ids.len()).unwrap_or(u64::MAX))
    }

    pub(crate) async fn maintain(
        &self,
        today: NaiveDate,
        keep_days: u32,
        ahead_days: u32,
        now: DateTime<Utc>,
    ) -> Res<PartitionReport> {
        let keep_from = today - Duration::days(i64::from(keep_days));
        let mut report = PartitionReport {
            expired_runs: self.expire_unfinished(keep_from, now).await?,
            ..PartitionReport::default()
        };
        for (table, pmax_rows) in TABLES.into_iter().zip(PMAX_ROWS) {
            let parts = self.partitions(table).await?;
            if parts.is_empty() {
                continue; // not partitioned (should not happen after the migration)
            }
            let highest = parts.iter().filter_map(|p| p.below).max();
            // A table with no day partitions yet starts at the oldest day kept (older rows are dropped next).
            let from = highest.unwrap_or(keep_from - Duration::days(1));
            let to = today + Duration::days(i64::from(ahead_days));
            if let Some(ddl) = reorganize_ddl(table, from, to)
                && self.ddl(&ddl).await?
            {
                report.created += u32::try_from((to - from).num_days() + 1).unwrap_or(0);
            }
            let parts = self.partitions(table).await?;
            let old = droppable(&parts, keep_from);
            if !old.is_empty() {
                let ddl = format!("ALTER TABLE {table} DROP PARTITION {}", old.join(", "));
                if self.ddl(&ddl).await? {
                    report.dropped += u32::try_from(old.len()).unwrap_or(0);
                }
            }
            let stray: i64 = sqlx::query_scalar(pmax_rows)
                .fetch_one(&self.pool)
                .await
                .map_err(db_err)?;
            report.rows_in_catch_all += u64::try_from(stray).unwrap_or(0);
        }
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn day_partitions_are_split_out_of_the_catch_all() {
        let ddl = reorganize_ddl("executions", day("2026-11-18"), day("2026-11-19")).unwrap();
        assert_eq!(
            ddl,
            "ALTER TABLE executions REORGANIZE PARTITION pmax INTO (\
             PARTITION p20261118 VALUES LESS THAN ('2026-11-19'), \
             PARTITION p20261119 VALUES LESS THAN ('2026-11-20'), \
             PARTITION pmax VALUES LESS THAN (MAXVALUE))"
        );
        assert_eq!(
            reorganize_ddl("executions", day("2026-11-20"), day("2026-11-19")),
            None
        );
    }

    #[test]
    fn only_whole_days_before_the_cap_are_dropped_and_never_the_catch_all() {
        let parts = vec![
            Partition {
                name: "p20261017".into(),
                below: Some(day("2026-10-18")),
            },
            Partition {
                name: "p20261018".into(),
                below: Some(day("2026-10-19")),
            },
            Partition {
                name: "p20261019".into(),
                below: Some(day("2026-10-20")),
            },
            Partition {
                name: "pmax".into(),
                below: None,
            },
        ];
        assert_eq!(
            droppable(&parts, day("2026-10-19")),
            ["p20261017", "p20261018"]
        );
        assert!(droppable(&parts, day("2026-01-01")).is_empty());
        assert_eq!(bound("'2026-11-19'"), Some(day("2026-11-19")));
        assert_eq!(bound("MAXVALUE"), None);
    }
}
