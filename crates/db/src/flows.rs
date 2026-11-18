//! `FlowRepo` for MySQL.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::types::Json;
use uuid::Uuid;
use weft_app::{
    AppError,
    ports::{FlowListRow, FlowRepo, Res},
};
use weft_domain::{
    error_codes,
    flow::FlowDefinition,
    ids::{ExecutionId, FlowId, FlowVersionId, OrgId},
    model::{Flow, FlowVersion, TriggerRegistration},
};

use crate::{
    MySqlStore,
    codec::{db_err, enum_str, parse_enum},
    rows::{FlowRow, VersionRow},
};

const FLOW_COLUMNS: &str = "f.id, f.org_id, f.name, f.description, f.is_enabled, f.draft_definition, f.draft_revision, \
    f.published_version_id, v.version AS published_version, f.created_at, f.updated_at, f.created_by";

#[derive(Debug, sqlx::FromRow)]
struct FlowListSql {
    #[sqlx(flatten)]
    flow: FlowRow,
    last_id: Option<Uuid>,
    last_status: Option<String>,
    last_queued_at: Option<DateTime<Utc>>,
}

#[async_trait]
impl FlowRepo for MySqlStore {
    async fn insert_flow(&self, flow: &Flow, reg: &TriggerRegistration) -> Res<()> {
        let mut tx = self.pool.begin().await.map_err(db_err)?;
        sqlx::query("INSERT INTO flows (id, org_id, name, description, is_enabled, draft_definition, draft_revision, created_at, updated_at, created_by) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(flow.id.as_uuid())
            .bind(flow.org_id.as_uuid())
            .bind(&flow.name)
            .bind(&flow.description)
            .bind(flow.is_enabled)
            .bind(Json(&flow.draft))
            .bind(flow.draft_revision)
            .bind(flow.created_at)
            .bind(flow.updated_at)
            .bind(flow.created_by.map(|u| u.as_uuid()))
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        sqlx::query("INSERT INTO trigger_registrations (id, org_id, flow_id, kind, token, secret, is_active, state, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(reg.id.as_uuid())
            .bind(reg.org_id.as_uuid())
            .bind(reg.flow_id.as_uuid())
            .bind(enum_str(&reg.kind))
            .bind(&reg.token)
            .bind(&reg.secret)
            .bind(reg.is_active)
            .bind(Json(&reg.state))
            .bind(flow.created_at)
            .bind(flow.created_at)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        tx.commit().await.map_err(db_err)
    }

    async fn get_flow(&self, org: OrgId, id: FlowId) -> Res<Option<Flow>> {
        let sql = format!(
            "SELECT {FLOW_COLUMNS} FROM flows f LEFT JOIN flow_versions v ON v.id = f.published_version_id WHERE f.org_id = ? AND f.id = ? AND f.deleted_at IS NULL"
        );
        let row: Option<FlowRow> = sqlx::query_as(&sql)
            .bind(org.as_uuid())
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(db_err)?;
        row.map(Flow::try_from).transpose()
    }

    async fn get_flow_system(&self, id: FlowId) -> Res<Option<Flow>> {
        let sql = format!(
            "SELECT {FLOW_COLUMNS} FROM flows f LEFT JOIN flow_versions v ON v.id = f.published_version_id WHERE f.id = ? AND f.deleted_at IS NULL"
        );
        let row: Option<FlowRow> = sqlx::query_as(&sql)
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(db_err)?;
        row.map(Flow::try_from).transpose()
    }

    async fn list_flows(&self, org: OrgId) -> Res<Vec<FlowListRow>> {
        let sql = format!(
            "SELECT {FLOW_COLUMNS}, \
             (SELECT e.id FROM executions e WHERE e.org_id = f.org_id AND e.flow_id = f.id ORDER BY e.queued_at DESC LIMIT 1) AS last_id, \
             (SELECT e.status FROM executions e WHERE e.org_id = f.org_id AND e.flow_id = f.id ORDER BY e.queued_at DESC LIMIT 1) AS last_status, \
             (SELECT e.queued_at FROM executions e WHERE e.org_id = f.org_id AND e.flow_id = f.id ORDER BY e.queued_at DESC LIMIT 1) AS last_queued_at \
             FROM flows f LEFT JOIN flow_versions v ON v.id = f.published_version_id \
             WHERE f.org_id = ? AND f.deleted_at IS NULL ORDER BY f.updated_at DESC LIMIT 500"
        );
        let rows: Vec<FlowListSql> = sqlx::query_as(&sql)
            .bind(org.as_uuid())
            .fetch_all(&self.pool)
            .await
            .map_err(db_err)?;
        rows.into_iter()
            .map(|r| {
                let last = match (r.last_id, r.last_status, r.last_queued_at) {
                    (Some(id), Some(status), Some(at)) => {
                        Some((ExecutionId::from_uuid(id), parse_enum(&status)?, at))
                    }
                    _ => None,
                };
                Ok(FlowListRow {
                    flow: Flow::try_from(r.flow)?,
                    last_execution: last,
                })
            })
            .collect()
    }

    async fn update_flow_meta(
        &self,
        org: OrgId,
        id: FlowId,
        name: &str,
        description: Option<&str>,
        now: DateTime<Utc>,
    ) -> Res<()> {
        sqlx::query("UPDATE flows SET name = ?, description = ?, updated_at = ? WHERE org_id = ? AND id = ? AND deleted_at IS NULL")
            .bind(name)
            .bind(description)
            .bind(now)
            .bind(org.as_uuid())
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(())
    }

    async fn save_draft(
        &self,
        org: OrgId,
        id: FlowId,
        def: &FlowDefinition,
        expected_revision: u32,
        now: DateTime<Utc>,
    ) -> Res<u32> {
        let result = sqlx::query("UPDATE flows SET draft_definition = ?, draft_revision = draft_revision + 1, updated_at = ? WHERE org_id = ? AND id = ? AND draft_revision = ? AND deleted_at IS NULL")
            .bind(Json(def))
            .bind(now)
            .bind(org.as_uuid())
            .bind(id.as_uuid())
            .bind(expected_revision)
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        if result.rows_affected() == 0 {
            return Err(AppError::conflict(
                error_codes::FLOW_REVISION_CONFLICT,
                "The flow was changed elsewhere; reload it",
            ));
        }
        Ok(expected_revision + 1)
    }

    async fn soft_delete_flow(&self, org: OrgId, id: FlowId, now: DateTime<Utc>) -> Res<()> {
        sqlx::query("UPDATE flows SET deleted_at = ?, is_enabled = FALSE, updated_at = ? WHERE org_id = ? AND id = ?")
            .bind(now)
            .bind(now)
            .bind(org.as_uuid())
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(())
    }

    async fn publish_version(&self, version: &FlowVersion, now: DateTime<Utc>) -> Res<u32> {
        let mut tx = self.pool.begin().await.map_err(db_err)?;
        // Lock the flow row so concurrent publishes get consecutive numbers.
        sqlx::query("SELECT id FROM flows WHERE id = ? AND org_id = ? FOR UPDATE")
            .bind(version.flow_id.as_uuid())
            .bind(version.org_id.as_uuid())
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        let max: Option<u32> = sqlx::query_scalar(
            "SELECT MAX(version) FROM flow_versions WHERE flow_id = ? AND kind = 'published'",
        )
        .bind(version.flow_id.as_uuid())
        .fetch_one(&mut *tx)
        .await
        .map_err(db_err)?;
        let number = max.unwrap_or(0) + 1;
        sqlx::query("INSERT INTO flow_versions (id, org_id, flow_id, kind, version, definition, created_at) VALUES (?, ?, ?, 'published', ?, ?, ?)")
            .bind(version.id.as_uuid())
            .bind(version.org_id.as_uuid())
            .bind(version.flow_id.as_uuid())
            .bind(number)
            .bind(Json(&version.definition))
            .bind(version.created_at)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        sqlx::query(
            "UPDATE flows SET published_version_id = ?, updated_at = ? WHERE id = ? AND org_id = ?",
        )
        .bind(version.id.as_uuid())
        .bind(now)
        .bind(version.flow_id.as_uuid())
        .bind(version.org_id.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;
        tx.commit().await.map_err(db_err)?;
        Ok(number)
    }

    async fn insert_test_snapshot(&self, version: &FlowVersion) -> Res<()> {
        sqlx::query("INSERT INTO flow_versions (id, org_id, flow_id, kind, version, definition, created_at) VALUES (?, ?, ?, 'test_snapshot', NULL, ?, ?)")
            .bind(version.id.as_uuid())
            .bind(version.org_id.as_uuid())
            .bind(version.flow_id.as_uuid())
            .bind(Json(&version.definition))
            .bind(version.created_at)
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(())
    }

    async fn get_version(&self, id: FlowVersionId) -> Res<Option<FlowVersion>> {
        let row: Option<VersionRow> = sqlx::query_as("SELECT id, org_id, flow_id, kind, version, definition, created_at FROM flow_versions WHERE id = ?")
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(db_err)?;
        row.map(FlowVersion::try_from).transpose()
    }

    async fn list_versions(&self, org: OrgId, flow: FlowId) -> Res<Vec<FlowVersion>> {
        let rows: Vec<VersionRow> = sqlx::query_as("SELECT id, org_id, flow_id, kind, version, definition, created_at FROM flow_versions WHERE org_id = ? AND flow_id = ? AND kind = 'published' ORDER BY version DESC LIMIT 200")
            .bind(org.as_uuid())
            .bind(flow.as_uuid())
            .fetch_all(&self.pool)
            .await
            .map_err(db_err)?;
        rows.into_iter().map(FlowVersion::try_from).collect()
    }

    async fn set_enabled(
        &self,
        org: OrgId,
        id: FlowId,
        enabled: bool,
        now: DateTime<Utc>,
    ) -> Res<()> {
        sqlx::query("UPDATE flows SET is_enabled = ?, updated_at = ? WHERE org_id = ? AND id = ? AND deleted_at IS NULL")
            .bind(enabled)
            .bind(now)
            .bind(org.as_uuid())
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(())
    }
}
