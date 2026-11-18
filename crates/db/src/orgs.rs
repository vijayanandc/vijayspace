//! `OrgRepo` for MySQL.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use weft_app::ports::{OrgRepo, Res};
use weft_domain::{ids::OrgId, model::Org};

use crate::{MySqlStore, codec::db_err, rows::OrgRow};

#[async_trait]
impl OrgRepo for MySqlStore {
    async fn ensure_default_org(&self, id: OrgId, now: DateTime<Utc>) -> Res<Org> {
        if let Some(org) = self.get_org(id).await? {
            return Ok(org);
        }
        let inserted = sqlx::query("INSERT INTO orgs (id, name, timezone, is_default, created_at, updated_at) VALUES (?, 'Default org', 'UTC', TRUE, ?, ?)")
            .bind(id.as_uuid())
            .bind(now)
            .bind(now)
            .execute(&self.pool)
            .await;
        if let Err(e) = inserted
            && !crate::codec::is_duplicate(&e)
        {
            return Err(db_err(e));
        }
        self.get_org(id)
            .await?
            .ok_or_else(|| weft_app::AppError::internal("default org missing"))
    }

    async fn get_org(&self, id: OrgId) -> Res<Option<Org>> {
        let row: Option<OrgRow> = sqlx::query_as(
            "SELECT id, name, timezone, retention_days, created_at FROM orgs WHERE id = ?",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(row.map(Org::from))
    }

    async fn list_orgs(&self) -> Res<Vec<Org>> {
        let rows: Vec<OrgRow> = sqlx::query_as(
            "SELECT id, name, timezone, retention_days, created_at FROM orgs ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(rows.into_iter().map(Org::from).collect())
    }

    async fn set_retention_days(
        &self,
        id: OrgId,
        days: Option<u32>,
        now: DateTime<Utc>,
    ) -> Res<()> {
        sqlx::query("UPDATE orgs SET retention_days = ?, updated_at = ? WHERE id = ?")
            .bind(days)
            .bind(now)
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(())
    }
}
