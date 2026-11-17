//! Ports: Connections (encrypted secrets) and pending OAuth authorizations.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use weft_domain::{
    ids::{ConnectionId, OrgId},
    model::{Connection, ConnectionStatus},
};

use super::Res;

/// A connection with its encrypted secrets.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredConnection {
    pub connection: Connection,
    pub ciphertext: Vec<u8>,
    pub key_id: String,
}

/// A pending OAuth authorization.
#[derive(Debug, Clone, PartialEq)]
pub struct OAuthState {
    pub state_hash: Vec<u8>,
    pub org_id: OrgId,
    pub connector: String,
    pub connection_id: Option<ConnectionId>,
    pub connection_name: String,
    pub config: Value,
    pub verifier_cipher: Option<Vec<u8>>,
    pub return_to: String,
    pub expires_at: DateTime<Utc>,
}

#[async_trait]
pub trait ConnectionRepo: Send + Sync {
    async fn insert_connection(&self, conn: &StoredConnection) -> Res<()>;
    async fn get_connection(&self, org: OrgId, id: ConnectionId) -> Res<Option<StoredConnection>>;
    async fn list_connections(&self, org: OrgId) -> Res<Vec<Connection>>;
    async fn update_connection(&self, conn: &StoredConnection) -> Res<()>;
    async fn set_connection_status(
        &self,
        id: ConnectionId,
        status: ConnectionStatus,
        now: DateTime<Utc>,
    ) -> Res<()>;
    async fn soft_delete_connection(
        &self,
        org: OrgId,
        id: ConnectionId,
        now: DateTime<Utc>,
    ) -> Res<()>;
    async fn insert_oauth_state(&self, state: &OAuthState, now: DateTime<Utc>) -> Res<()>;
    /// Removes and returns a state (single use) if it has not expired.
    async fn take_oauth_state(
        &self,
        state_hash: &[u8],
        now: DateTime<Utc>,
    ) -> Res<Option<OAuthState>>;
}
