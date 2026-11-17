//! `TriggerRepo` for MySQL: registrations and provider subscriptions.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{QueryBuilder, types::Json};
use weft_app::ports::{Res, Subscription, TriggerRepo};
use weft_domain::{
    ids::{FlowId, RegistrationId},
    model::TriggerRegistration,
};

use crate::{
    MySqlStore,
    codec::{db_err, enum_str},
    rows::{RegistrationRow, SubscriptionRow},
};

const REG_COLUMNS: &str = "id, org_id, flow_id, kind, token, secret, is_active, listening_until, next_fire_at, state, sample";
const SUB_COLUMNS: &str = "id, org_id, registration_id, connection_id, connector_key, external_id, data, status, expires_at, renew_after, last_error";

async fn one(
    store: &MySqlStore,
    sql: &str,
    bind: sqlx::query::QueryAs<'_, sqlx::MySql, RegistrationRow, sqlx::mysql::MySqlArguments>,
) -> Res<Option<TriggerRegistration>> {
    let _ = sql;
    let row: Option<RegistrationRow> = bind.fetch_optional(&store.pool).await.map_err(db_err)?;
    row.map(TriggerRegistration::try_from).transpose()
}

#[async_trait]
impl TriggerRepo for MySqlStore {
    async fn get_registration_by_flow(&self, flow: FlowId) -> Res<Option<TriggerRegistration>> {
        let sql = format!("SELECT {REG_COLUMNS} FROM trigger_registrations WHERE flow_id = ?");
        one(self, &sql, sqlx::query_as(&sql).bind(flow.as_uuid())).await
    }

    async fn get_registration_by_token(&self, token: &str) -> Res<Option<TriggerRegistration>> {
        let sql = format!("SELECT {REG_COLUMNS} FROM trigger_registrations WHERE token = ?");
        one(self, &sql, sqlx::query_as(&sql).bind(token)).await
    }

    async fn get_registration(&self, id: RegistrationId) -> Res<Option<TriggerRegistration>> {
        let sql = format!("SELECT {REG_COLUMNS} FROM trigger_registrations WHERE id = ?");
        one(self, &sql, sqlx::query_as(&sql).bind(id.as_uuid())).await
    }

    async fn update_registration(&self, reg: &TriggerRegistration, now: DateTime<Utc>) -> Res<()> {
        sqlx::query("UPDATE trigger_registrations SET kind = ?, is_active = ?, next_fire_at = ?, state = ?, updated_at = ? WHERE id = ?")
            .bind(enum_str(&reg.kind))
            .bind(reg.is_active)
            .bind(reg.next_fire_at)
            .bind(Json(&reg.state))
            .bind(now)
            .bind(reg.id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(())
    }

    async fn set_listening(
        &self,
        id: RegistrationId,
        until: Option<DateTime<Utc>>,
        now: DateTime<Utc>,
    ) -> Res<()> {
        sqlx::query(
            "UPDATE trigger_registrations SET listening_until = ?, updated_at = ? WHERE id = ?",
        )
        .bind(until)
        .bind(now)
        .bind(id.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn set_sample(&self, id: RegistrationId, sample: &Value, now: DateTime<Utc>) -> Res<()> {
        sqlx::query("UPDATE trigger_registrations SET sample = ?, updated_at = ? WHERE id = ?")
            .bind(Json(sample))
            .bind(now)
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(())
    }

    async fn set_state(&self, id: RegistrationId, state: &Value, now: DateTime<Utc>) -> Res<()> {
        sqlx::query("UPDATE trigger_registrations SET state = ?, updated_at = ? WHERE id = ?")
            .bind(Json(state))
            .bind(now)
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(())
    }

    async fn take_due(
        &self,
        kinds: &[&str],
        now: DateTime<Utc>,
        lease_until: DateTime<Utc>,
        limit: u32,
    ) -> Res<Vec<TriggerRegistration>> {
        if kinds.is_empty() {
            return Ok(Vec::new());
        }
        let mut tx = self.pool.begin().await.map_err(db_err)?;
        let mut qb = QueryBuilder::new(format!(
            "SELECT {REG_COLUMNS} FROM trigger_registrations WHERE is_active = TRUE AND next_fire_at <= "
        ));
        qb.push_bind(now).push(" AND kind IN (");
        let mut sep = qb.separated(", ");
        for k in kinds {
            sep.push_bind(*k);
        }
        qb.push(") ORDER BY next_fire_at LIMIT ")
            .push_bind(limit)
            .push(" FOR UPDATE SKIP LOCKED");
        let rows: Vec<RegistrationRow> = qb
            .build_query_as()
            .fetch_all(&mut *tx)
            .await
            .map_err(db_err)?;
        for row in &rows {
            sqlx::query(
                "UPDATE trigger_registrations SET next_fire_at = ?, updated_at = ? WHERE id = ?",
            )
            .bind(lease_until)
            .bind(now)
            .bind(row.id)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        }
        tx.commit().await.map_err(db_err)?;
        rows.into_iter()
            .map(TriggerRegistration::try_from)
            .collect()
    }

    async fn set_next_fire(
        &self,
        id: RegistrationId,
        next: Option<DateTime<Utc>>,
        now: DateTime<Utc>,
    ) -> Res<()> {
        sqlx::query(
            "UPDATE trigger_registrations SET next_fire_at = ?, updated_at = ? WHERE id = ?",
        )
        .bind(next)
        .bind(now)
        .bind(id.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn upsert_subscription(&self, sub: &Subscription, now: DateTime<Utc>) -> Res<()> {
        sqlx::query(
            "INSERT INTO app_subscriptions (id, org_id, registration_id, connection_id, connector_key, external_id, data, status, expires_at, renew_after, last_error, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) AS new \
             ON DUPLICATE KEY UPDATE connection_id = new.connection_id, connector_key = new.connector_key, external_id = new.external_id, data = new.data, \
             status = new.status, expires_at = new.expires_at, renew_after = new.renew_after, last_error = new.last_error, updated_at = new.updated_at",
        )
        .bind(sub.id.as_uuid())
        .bind(sub.org_id.as_uuid())
        .bind(sub.registration_id.as_uuid())
        .bind(sub.connection_id.as_uuid())
        .bind(&sub.connector)
        .bind(&sub.external_id)
        .bind(Json(&sub.data))
        .bind(&sub.status)
        .bind(sub.expires_at)
        .bind(sub.renew_after)
        .bind(&sub.last_error)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn get_subscription(&self, registration: RegistrationId) -> Res<Option<Subscription>> {
        let sql = format!("SELECT {SUB_COLUMNS} FROM app_subscriptions WHERE registration_id = ?");
        let row: Option<SubscriptionRow> = sqlx::query_as(&sql)
            .bind(registration.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(row.map(Subscription::from))
    }

    async fn delete_subscription(&self, registration: RegistrationId) -> Res<()> {
        sqlx::query("DELETE FROM app_subscriptions WHERE registration_id = ?")
            .bind(registration.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(())
    }

    async fn due_subscriptions(&self, now: DateTime<Utc>, limit: u32) -> Res<Vec<Subscription>> {
        let sql = format!(
            "SELECT {SUB_COLUMNS} FROM app_subscriptions WHERE renew_after IS NOT NULL AND renew_after <= ? ORDER BY renew_after LIMIT ?"
        );
        let rows: Vec<SubscriptionRow> = sqlx::query_as(&sql)
            .bind(now)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(rows.into_iter().map(Subscription::from).collect())
    }
}
