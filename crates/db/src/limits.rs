//! `LimitRepo` for MySQL: connection limit overrides and atomic daily request counters.

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Timelike as _, Utc};
use sqlx::types::Json;
use uuid::Uuid;
use weft_app::ports::{ConnectionUsage, Counted, LimitRepo, Res};
use weft_connector_sdk::ConnectionLimits;
use weft_domain::ids::{ConnectionId, OrgId};

use crate::{MySqlStore, codec::db_err};

#[async_trait]
impl LimitRepo for MySqlStore {
    async fn get_connection_limits(&self, org: OrgId, id: ConnectionId) -> Res<ConnectionLimits> {
        let row: Option<Option<Json<ConnectionLimits>>> = sqlx::query_scalar(
            "SELECT limits FROM connections WHERE org_id = ? AND id = ? AND deleted_at IS NULL",
        )
        .bind(org.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(row.flatten().map(|j| j.0).unwrap_or_default())
    }

    async fn set_connection_limits(
        &self,
        org: OrgId,
        id: ConnectionId,
        overrides: &ConnectionLimits,
        now: DateTime<Utc>,
    ) -> Res<()> {
        sqlx::query("UPDATE connections SET limits = ?, updated_at = ? WHERE org_id = ? AND id = ? AND deleted_at IS NULL")
            .bind(Json(overrides))
            .bind(now)
            .bind(org.as_uuid())
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(())
    }

    async fn count_connection_request(
        &self,
        org: OrgId,
        id: ConnectionId,
        daily_cap: Option<u32>,
        per_minute: Option<u32>,
        now: DateTime<Utc>,
    ) -> Res<Counted> {
        let day = now.date_naive();
        let minute = minute_of(now);
        sqlx::query("INSERT IGNORE INTO connection_usage (org_id, connection_id, day, requests, refused, updated_at) VALUES (?, ?, ?, 0, 0, ?)")
            .bind(org.as_uuid())
            .bind(id.as_uuid())
            .bind(day)
            .bind(now)
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        // The row lock serialises concurrent requests from every server; each re-checks both caps on the latest
        // counts. Assignments run left to right: the minute count is reset before `minute_start` moves on.
        let counted = sqlx::query(
            "UPDATE connection_usage SET minute_requests = IF(minute_start <=> ?, minute_requests + 1, 1), \
             minute_start = ?, requests = requests + 1, updated_at = ? \
             WHERE org_id = ? AND connection_id = ? AND day = ? AND (? IS NULL OR requests < ?) \
             AND (? IS NULL OR NOT (minute_start <=> ?) OR minute_requests < ?)",
        )
        .bind(minute)
        .bind(minute)
        .bind(now)
        .bind(org.as_uuid())
        .bind(id.as_uuid())
        .bind(day)
        .bind(daily_cap)
        .bind(daily_cap)
        .bind(per_minute)
        .bind(minute)
        .bind(per_minute)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        if counted.rows_affected() > 0 {
            return Ok(Counted::Allowed);
        }
        // Which cap refused: the day's count only grows, so it is decided from that alone. The minute window may
        // already have moved on (another server counted in the next minute), which must not turn a minute refusal
        // into a wait until midnight.
        let requests: u32 = sqlx::query_scalar(
            "SELECT requests FROM connection_usage WHERE org_id = ? AND connection_id = ? AND day = ?",
        )
        .bind(org.as_uuid())
        .bind(id.as_uuid())
        .bind(day)
        .fetch_one(&self.pool)
        .await
        .map_err(db_err)?;
        sqlx::query("UPDATE connection_usage SET refused = refused + 1, updated_at = ? WHERE org_id = ? AND connection_id = ? AND day = ?")
            .bind(now)
            .bind(org.as_uuid())
            .bind(id.as_uuid())
            .bind(day)
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        let day_full = daily_cap.is_some_and(|cap| requests >= cap);
        Ok(if day_full || per_minute.is_none() {
            Counted::DayFull
        } else {
            Counted::MinuteFull
        })
    }

    async fn connection_usage(&self, org: OrgId, day: NaiveDate) -> Res<Vec<ConnectionUsage>> {
        let rows: Vec<(Uuid, u32, u32)> = sqlx::query_as(
            "SELECT connection_id, requests, refused FROM connection_usage WHERE org_id = ? AND day = ?",
        )
        .bind(org.as_uuid())
        .bind(day)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(rows
            .into_iter()
            .map(|(id, requests, refused)| ConnectionUsage {
                connection_id: ConnectionId::from_uuid(id),
                requests,
                refused,
            })
            .collect())
    }
}

/// The start of `now`'s UTC minute (the per-minute window).
fn minute_of(now: DateTime<Utc>) -> DateTime<Utc> {
    now.with_second(0)
        .and_then(|t| t.with_nanosecond(0))
        .unwrap_or(now)
}
