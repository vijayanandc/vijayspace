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
    /// Who started the sign-in (becomes the connection's creator).
    pub created_by: Option<weft_domain::ids::UserId>,
    /// The provider's callback (code and parameters), encrypted, once the browser came back.
    pub callback_cipher: Option<Vec<u8>>,
    /// SHA-256 of the one-time token that finishes this sign-in.
    pub finish_hash: Option<Vec<u8>>,
    /// The connection's own OAuth client secret (apps where each connection brings its client), encrypted.
    pub client_cipher: Option<Vec<u8>>,
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
    /// Keeps the provider's callback with a finish token, once, on an open sign-in; returns it.
    async fn park_oauth_callback(
        &self,
        state_hash: &[u8],
        callback_cipher: &[u8],
        finish_hash: &[u8],
        now: DateTime<Utc>,
    ) -> Res<Option<OAuthState>>;
    async fn take_oauth_state(
        &self,
        state_hash: &[u8],
        now: DateTime<Utc>,
    ) -> Res<Option<OAuthState>>;
    /// Takes (deletes and returns) a parked sign-in only for its owner: started in `org` by `user`, with the
    /// provider's answer parked under `finish_hash`. Anyone else gets `None` and the sign-in stays for its owner.
    async fn take_parked_oauth_state(
        &self,
        state_hash: &[u8],
        org: weft_domain::ids::OrgId,
        user: Option<weft_domain::ids::UserId>,
        finish_hash: &[u8],
        now: DateTime<Utc>,
    ) -> Res<Option<OAuthState>>;
}
