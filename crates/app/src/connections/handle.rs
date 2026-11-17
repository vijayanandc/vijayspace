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
use weft_domain::model::ConnectionStatus;

use super::secret_aad;
use crate::{Deps, ports::StoredConnection};

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
        let secrets = decrypt(&self.deps, &stored)?;
        let now = self.deps.clock.now();
        let current = token_of(&secrets).ok();
        // A forced refresh (after a 401) is skipped when the token already changed since this
        // handle read it: N concurrent 401s cause one refresh, not N (single-use refresh tokens
        // would otherwise invalidate each other).
        let refreshed_meanwhile = force && current.is_some() && current != seen;
        if refreshed_meanwhile || (!force && !oauth::needs_refresh(&secrets, now)) {
            return token_of(&secrets);
        }
        let (connector, spec, client) = oauth_setup(&self.deps, &stored.connection.connector)?;
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
        let new_secrets = tokens.to_secrets(Some(&refresh_token));
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
            .update_connection(&stored)
            .await
            .map_err(|e| ConnectorError::transient(e.to_string()))?;
        token_of(&new_secrets)
    }
}

/// The connector, its OAuth settings and this server's OAuth app for it.
fn oauth_setup(
    deps: &Deps,
    connector_key: &str,
) -> Result<(Arc<dyn Connector>, OAuth2Spec, OAuthClient), ConnectorError> {
    let connector = deps
        .connectors
        .get(connector_key)
        .cloned()
        .ok_or_else(|| ConnectorError::permanent("unknown connector"))?;
    let spec = connector
        .manifest()
        .auth
        .oauth2
        .clone()
        .ok_or_else(|| ConnectorError::permanent("connector has no OAuth settings"))?;
    let client = deps
        .config
        .oauth_clients
        .get(connector_key)
        .cloned()
        .ok_or_else(|| {
            ConnectorError::permanent(format!(
                "OAuth app for `{connector_key}` is not configured on this server"
            ))
        })?;
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
