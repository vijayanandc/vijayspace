//! `LimitRepo` for MySQL: connection limit overrides and atomic daily request counters.

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::types::Json;
use uuid::Uuid;
use weft_app::ports::{ConnectionUsage, LimitRepo, Res};
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
        day: NaiveDate,
        cap: Option<u32>,
        now: DateTime<Utc>,
    ) -> Res<bool> {
        sqlx::query("INSERT IGNORE INTO connection_usage (org_id, connection_id, day, requests, refused, updated_at) VALUES (?, ?, ?, 0, 0, ?)")
            .bind(org.as_uuid())
            .bind(id.as_uuid())
            .bind(day)
            .bind(now)
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        // The row lock serialises concurrent requests; each re-checks the cap on the latest count.
        let counted = sqlx::query("UPDATE connection_usage SET requests = requests + 1, updated_at = ? WHERE connection_id = ? AND day = ? AND (? IS NULL OR requests < ?)")
            .bind(now)
            .bind(id.as_uuid())
            .bind(day)
            .bind(cap)
            .bind(cap)
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        if counted.rows_affected() > 0 {
            return Ok(true);
        }
        sqlx::query("UPDATE connection_usage SET refused = refused + 1, updated_at = ? WHERE connection_id = ? AND day = ?")
            .bind(now)
            .bind(id.as_uuid())
            .bind(day)
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(false)
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
