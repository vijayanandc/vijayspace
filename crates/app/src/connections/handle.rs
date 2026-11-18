//! `ConnectionHandle` for stored connections: decrypts secrets and refreshes OAuth tokens.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use serde_json::{Map, Value};
use weft_connector_sdk::{
    ConnectionHandle, Connector, ConnectorError, OAuth2Spec,
    oauth::{self, OAuthClient},
};
use weft_domain::{
    ids::{ConnectionId, OrgId},
    model::ConnectionStatus,
};

use super::secret_aad;
use crate::{Deps, ports::StoredConnection};

/// How long one server may hold a connection's refresh lease (a token call takes seconds).
const REFRESH_LEASE_SECS: i64 = 60;
/// While another server refreshes, look for its new token this often, at most this many times (15 s).
const REFRESH_POLL: std::time::Duration = std::time::Duration::from_millis(250);
const REFRESH_POLLS: u32 = 60;

/// The refresh lease of a connection in `sync_leases` (32 bytes: a tag and the connection ID).
#[must_use]
pub fn refresh_lease_key(id: ConnectionId) -> [u8; 32] {
    let mut key = [0_u8; 32];
    key[..13].copy_from_slice(b"oauth-refresh");
    key[16..].copy_from_slice(id.as_uuid().as_bytes());
    key
}

/// Per-connection async locks so only one refresh runs at a time in this process.
#[derive(Debug, Default)]
pub struct RefreshLocks {
    locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

impl RefreshLocks {
    fn lock_for(&self, key: &str) -> Arc<tokio::sync::Mutex<()>> {
        self.locks.lock().map_or_else(
            |_| Arc::new(tokio::sync::Mutex::new(())),
            |mut m| m.entry(key.to_owned()).or_default().clone(),
        )
    }
}

/// A stored connection, usable by connectors.
pub struct DbConnectionHandle {
    deps: Deps,
    stored: tokio::sync::Mutex<StoredConnection>,
    id: String,
    config: Value,
}

impl std::fmt::Debug for DbConnectionHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DbConnectionHandle")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

impl DbConnectionHandle {
    #[must_use]
    pub fn new(deps: Deps, stored: StoredConnection) -> Self {
        let id = stored.connection.id.to_string();
        let config = stored.connection.config.clone();
        Self {
            deps,
            stored: tokio::sync::Mutex::new(stored),
            id,
            config,
        }
    }

    async fn decrypt(&self) -> Result<Value, ConnectorError> {
        let stored = self.stored.lock().await;
        decrypt(&self.deps, &stored)
    }

    async fn refresh(&self, force: bool) -> Result<String, ConnectorError> {
        let lock = self.deps.refresh_locks.lock_for(&self.id);
        let _guard = lock.lock().await;
        // Re-read under the lock: another task may have refreshed already.
        let mut stored = self.stored.lock().await;
        let seen = decrypt(&self.deps, &stored)
            .ok()
            .and_then(|s| token_of(&s).ok());
        // One refresh per connection across every server (single-use refresh tokens would otherwise
        // invalidate each other): a server that finds the lease taken waits for the other's new token.
        let (org, key) = (
            stored.connection.org_id,
            refresh_lease_key(stored.connection.id),
        );
        let holder = format!("refresh-{}", self.deps.ids.uuid().simple());
        let mut polls = 0;
        while !self.take_lease(org, &key, &holder).await? {
            if let Some(token) = self.reload_if_changed(&mut stored, seen.as_deref()).await? {
                return Ok(token);
            }
            polls += 1;
            if polls >= REFRESH_POLLS {
                return Err(ConnectorError::transient(
                    "another server is refreshing this connection's token",
                ));
            }
            tokio::time::sleep(REFRESH_POLL).await;
        }
        let result = self.refresh_leased(&mut stored, seen, force).await;
        let _ = self.deps.store.release_lease(org, &key, &holder).await;
        result
    }

    async fn take_lease(
        &self,
        org: OrgId,
        key: &[u8; 32],
        holder: &str,
    ) -> Result<bool, ConnectorError> {
        let now = self.deps.clock.now();
        self.deps
            .store
            .acquire_lease(
                org,
                key,
                holder,
                now + chrono::Duration::seconds(REFRESH_LEASE_SECS),
                now,
            )
            .await
            .map_err(|e| ConnectorError::transient(e.to_string()))
    }

    /// Re-reads the connection; returns its token when it differs from `seen` (another server refreshed).
    async fn reload_if_changed(
        &self,
        stored: &mut StoredConnection,
        seen: Option<&str>,
    ) -> Result<Option<String>, ConnectorError> {
        let conn = &stored.connection;
        let Some(fresh) = self
            .deps
            .store
            .get_connection(conn.org_id, conn.id)
            .await
            .map_err(|e| ConnectorError::transient(e.to_string()))?
        else {
            return Err(ConnectorError::AuthExpired {
                message: "connection deleted".into(),
            });
        };
        let token = decrypt(&self.deps, &fresh)
            .ok()
            .and_then(|s| token_of(&s).ok());
        if token.is_none() || token.as_deref() == seen {
            return Ok(None);
        }
        *stored = fresh;
        Ok(token)
    }

    /// The refresh itself, with this server holding the connection's refresh lease.
    async fn refresh_leased(
        &self,
        stored: &mut StoredConnection,
        seen: Option<String>,
        force: bool,
    ) -> Result<String, ConnectorError> {
        let conn = &stored.connection;
        let fresh = self
            .deps
            .store
            .get_connection(conn.org_id, conn.id)
            .await
            .map_err(|e| ConnectorError::transient(e.to_string()))?
            .ok_or_else(|| ConnectorError::AuthExpired {
                message: "connection deleted".into(),
            })?;
        *stored = fresh;
        let secrets = decrypt(&self.deps, stored)?;
        let now = self.deps.clock.now();
        let current = token_of(&secrets).ok();
        // A forced refresh (after a 401) is skipped when the token already changed since this
        // handle read it: N concurrent 401s cause one refresh, not N (single-use refresh tokens
        // would otherwise invalidate each other).
        let refreshed_meanwhile = force && current.is_some() && current != seen;
        if refreshed_meanwhile || (!force && !oauth::needs_refresh(&secrets, now)) {
            return token_of(&secrets);
        }
        let (connector, spec, client) = oauth_setup(&self.deps, &stored.connection, &secrets)?;
        let refresh_token = secrets
            .get("refresh_token")
            .and_then(Value::as_str)
            .ok_or_else(|| ConnectorError::AuthExpired {
                message: "no refresh token; reconnect".into(),
            })?
            .to_owned();
        let tokens = match oauth::refresh(
            self.deps.http.as_ref(),
            &spec,
            &stored.connection.config,
            &client,
            &refresh_token,
            now,
        )
        .await
        {
            Ok(t) => t,
            Err(err) => {
                if matches!(err, ConnectorError::AuthExpired { .. }) {
                    let _ = self
                        .deps
                        .store
                        .set_connection_status(
                            stored.connection.id,
                            ConnectionStatus::NeedsReauth,
                            now,
                        )
                        .await;
                }
                return Err(err);
            }
        };
        let mut new_secrets = tokens.to_secrets(Some(&refresh_token));
        // A connection with its own OAuth client keeps its secret next to the tokens.
        if let (Some(secret), Value::Object(map)) = (secrets.get("client_secret"), &mut new_secrets)
        {
            map.insert("client_secret".into(), secret.clone());
        }
        let patch = connector.oauth_token_config(&tokens.raw);
        if let (Some(cfg), Value::Object(p)) = (stored.connection.config.as_object_mut(), patch) {
            cfg.extend(p.into_iter().filter(|(_, v)| !v.is_null()));
        }
        let aad = secret_aad(stored.connection.org_id, stored.connection.id);
        let (ciphertext, key_id) = self
            .deps
            .secrets
            .seal(new_secrets.to_string().as_bytes(), &aad)
            .map_err(|e| ConnectorError::permanent(e.to_string()))?;
        stored.ciphertext = ciphertext;
        stored.key_id = key_id;
        stored.connection.status = ConnectionStatus::Active;
        stored.connection.updated_at = now;
        self.deps
            .store
            .update_connection(stored)
            .await
            .map_err(|e| ConnectorError::transient(e.to_string()))?;
        token_of(&new_secrets)
    }
}

/// The connector, its OAuth settings for this connection, and the OAuth client to refresh with (the server's
/// app for the connector, or the connection's own client).
fn oauth_setup(
    deps: &Deps,
    connection: &weft_domain::model::Connection,
    secrets: &Value,
) -> Result<(Arc<dyn Connector>, OAuth2Spec, OAuthClient), ConnectorError> {
    let connector = deps
        .connectors
        .get(&connection.connector)
        .cloned()
        .ok_or_else(|| ConnectorError::permanent("unknown connector"))?;
    let spec = connector
        .oauth_spec(&connection.config)?
        .ok_or_else(|| ConnectorError::permanent("connector has no OAuth settings"))?;
    let own = secrets.get("client_secret").and_then(Value::as_str);
    let client = super::oauth::client_for(
        deps,
        connector.as_ref(),
        &connection.connector,
        &connection.config,
        own,
    )
    .map_err(ConnectorError::permanent)?;
    Ok((connector, spec, client))
}

fn token_of(secrets: &Value) -> Result<String, ConnectorError> {
    secrets
        .get("access_token")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| ConnectorError::AuthExpired {
            message: "no access token; reconnect".into(),
        })
}

/// Decrypts a stored connection's secrets.
pub fn decrypt(deps: &Deps, stored: &StoredConnection) -> Result<Value, ConnectorError> {
    let aad = secret_aad(stored.connection.org_id, stored.connection.id);
    let plain = deps
        .secrets
        .open(&stored.ciphertext, &stored.key_id, &aad)
        .map_err(|e| {
            ConnectorError::permanent(format!("cannot decrypt connection secrets: {e}"))
        })?;
    serde_json::from_slice(&plain)
        .map_err(|e| ConnectorError::permanent(format!("corrupt connection secrets: {e}")))
}

/// Encrypts a secrets object for a connection.
pub fn encrypt(
    deps: &Deps,
    conn: &weft_domain::model::Connection,
    secrets: &Map<String, Value>,
) -> Result<(Vec<u8>, String), crate::AppError> {
    deps.secrets.seal(
        Value::Object(secrets.clone()).to_string().as_bytes(),
        &secret_aad(conn.org_id, conn.id),
    )
}

#[async_trait]
impl ConnectionHandle for DbConnectionHandle {
    fn id(&self) -> &str {
        &self.id
    }

    fn config(&self) -> &Value {
        &self.config
    }

    async fn secrets(&self) -> Result<Value, ConnectorError> {
        self.decrypt().await
    }

    async fn access_token(&self) -> Result<String, ConnectorError> {
        let secrets = self.decrypt().await?;
        if oauth::needs_refresh(&secrets, self.deps.clock.now())
            && secrets.get("refresh_token").is_some()
        {
            return self.refresh(false).await;
        }
        token_of(&secrets)
    }

    async fn force_refresh(&self) -> Result<String, ConnectorError> {
        self.refresh(true).await
    }
}
