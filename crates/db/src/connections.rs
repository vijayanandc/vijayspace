//! `ConnectionRepo` for MySQL.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::types::Json;
use weft_app::ports::{ConnectionRepo, OAuthState, Res, StoredConnection};
use weft_domain::{
    ids::{ConnectionId, OrgId},
    model::{Connection, ConnectionStatus},
};

use crate::{
    MySqlStore,
    codec::{db_err, enum_str},
    rows::ConnectionRow,
};

const COLUMNS: &str = "id, org_id, connector_key, name, config, secret_ciphertext, secret_key_id, status, created_at, updated_at";

#[derive(Debug, sqlx::FromRow)]
struct StateRow {
    state_hash: Vec<u8>,
    org_id: uuid::Uuid,
    connector_key: String,
    connection_id: Option<uuid::Uuid>,
    connection_name: String,
    config: Json<serde_json::Value>,
    verifier_cipher: Option<Vec<u8>>,
    return_to: String,
    expires_at: DateTime<Utc>,
}

#[async_trait]
impl ConnectionRepo for MySqlStore {
    async fn insert_connection(&self, c: &StoredConnection) -> Res<()> {
        let conn = &c.connection;
        sqlx::query(&format!(
            "INSERT INTO connections ({COLUMNS}) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        ))
        .bind(conn.id.as_uuid())
        .bind(conn.org_id.as_uuid())
        .bind(&conn.connector)
        .bind(&conn.name)
        .bind(Json(&conn.config))
        .bind(&c.ciphertext)
        .bind(&c.key_id)
        .bind(enum_str(&conn.status))
        .bind(conn.created_at)
        .bind(conn.updated_at)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn get_connection(&self, org: OrgId, id: ConnectionId) -> Res<Option<StoredConnection>> {
        let row: Option<ConnectionRow> = sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM connections WHERE org_id = ? AND id = ? AND deleted_at IS NULL"
        ))
        .bind(org.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?;
        row.map(StoredConnection::try_from).transpose()
    }

    async fn list_connections(&self, org: OrgId) -> Res<Vec<Connection>> {
        let rows: Vec<ConnectionRow> = sqlx::query_as(&format!("SELECT {COLUMNS} FROM connections WHERE org_id = ? AND deleted_at IS NULL ORDER BY name LIMIT 500"))
            .bind(org.as_uuid())
            .fetch_all(&self.pool)
            .await
            .map_err(db_err)?;
        rows.into_iter()
            .map(|r| StoredConnection::try_from(r).map(|s| s.connection))
            .collect()
    }

    async fn update_connection(&self, c: &StoredConnection) -> Res<()> {
        let conn = &c.connection;
        sqlx::query("UPDATE connections SET name = ?, config = ?, secret_ciphertext = ?, secret_key_id = ?, status = ?, updated_at = ? WHERE org_id = ? AND id = ?")
            .bind(&conn.name)
            .bind(Json(&conn.config))
            .bind(&c.ciphertext)
            .bind(&c.key_id)
            .bind(enum_str(&conn.status))
            .bind(conn.updated_at)
            .bind(conn.org_id.as_uuid())
            .bind(conn.id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(())
    }

    async fn set_connection_status(
        &self,
        id: ConnectionId,
        status: ConnectionStatus,
        now: DateTime<Utc>,
    ) -> Res<()> {
        sqlx::query("UPDATE connections SET status = ?, updated_at = ? WHERE id = ?")
            .bind(enum_str(&status))
            .bind(now)
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(())
    }

    async fn soft_delete_connection(
        &self,
        org: OrgId,
        id: ConnectionId,
        now: DateTime<Utc>,
    ) -> Res<()> {
        sqlx::query(
            "UPDATE connections SET deleted_at = ?, updated_at = ? WHERE org_id = ? AND id = ?",
        )
        .bind(now)
        .bind(now)
        .bind(org.as_uuid())
        .bind(id.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn insert_oauth_state(&self, s: &OAuthState, now: DateTime<Utc>) -> Res<()> {
        sqlx::query("DELETE FROM oauth_states WHERE expires_at < ?")
            .bind(now)
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        sqlx::query(
            "INSERT INTO oauth_states (state_hash, org_id, connector_key, connection_id, connection_name, config, verifier_cipher, return_to, expires_at, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&s.state_hash)
        .bind(s.org_id.as_uuid())
        .bind(&s.connector)
        .bind(s.connection_id.map(|c| c.as_uuid()))
        .bind(&s.connection_name)
        .bind(Json(&s.config))
        .bind(&s.verifier_cipher)
        .bind(&s.return_to)
        .bind(s.expires_at)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn take_oauth_state(
        &self,
        state_hash: &[u8],
        now: DateTime<Utc>,
    ) -> Res<Option<OAuthState>> {
        let mut tx = self.pool.begin().await.map_err(db_err)?;
        let row: Option<StateRow> = sqlx::query_as(
            "SELECT state_hash, org_id, connector_key, connection_id, connection_name, config, verifier_cipher, return_to, expires_at FROM oauth_states WHERE state_hash = ? FOR UPDATE",
        )
        .bind(state_hash)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_err)?;
        sqlx::query("DELETE FROM oauth_states WHERE state_hash = ?")
            .bind(state_hash)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        tx.commit().await.map_err(db_err)?;
        Ok(row.filter(|r| r.expires_at > now).map(|r| OAuthState {
            state_hash: r.state_hash,
            org_id: OrgId::from_uuid(r.org_id),
            connector: r.connector_key,
            connection_id: r.connection_id.map(ConnectionId::from_uuid),
            connection_name: r.connection_name,
            config: r.config.0,
            verifier_cipher: r.verifier_cipher,
            return_to: r.return_to,
            expires_at: r.expires_at,
        }))
    }
}
