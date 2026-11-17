//! What a connector call can use: HTTP, the connection (credentials), and the mailer.

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{error::ConnectorError, http::HttpSender};

/// Access to one stored connection. Implemented by the platform (encrypted storage, token refresh).
#[async_trait]
pub trait ConnectionHandle: Send + Sync {
    /// The connection's prefixed ID.
    fn id(&self) -> &str;
    /// Non-secret config (base URL, data centre, API domain, region, org ID, …).
    fn config(&self) -> &Value;
    /// Decrypted secret fields (API tokens, passwords) or the OAuth token set.
    async fn secrets(&self) -> Result<Value, ConnectorError>;
    /// A valid OAuth access token, refreshed when it expires within two minutes.
    async fn access_token(&self) -> Result<String, ConnectorError>;
    /// Refreshes the OAuth token now (after a 401) and returns the new access token.
    async fn force_refresh(&self) -> Result<String, ConnectorError>;
}

/// An email to send through the platform mailer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmailMessage {
    pub to: Vec<String>,
    #[serde(default)]
    pub cc: Vec<String>,
    pub subject: String,
    pub body: String,
    #[serde(default)]
    pub html: bool,
    #[serde(default)]
    pub reply_to: Option<String>,
}

/// Sends email (SMTP in production, a capturing fake in tests).
#[async_trait]
pub trait Mailer: Send + Sync {
    async fn send(&self, message: &EmailMessage) -> Result<(), ConnectorError>;
}

/// Everything one action call may use.
#[derive(Clone)]
pub struct ActionCtx {
    pub org_id: String,
    pub http: Arc<dyn HttpSender>,
    pub connection: Option<Arc<dyn ConnectionHandle>>,
    /// `sha256(execution ‖ step ‖ iteration)`: forward it where the app supports idempotency keys.
    pub idempotency_key: String,
    pub mailer: Option<Arc<dyn Mailer>>,
    pub timeout: Duration,
    /// The platform clock's current time (connectors never read the wall clock directly).
    pub now: DateTime<Utc>,
}

impl ActionCtx {
    /// The connection, or an error telling the user to choose one.
    pub fn conn(&self) -> Result<&dyn ConnectionHandle, ConnectorError> {
        self.connection.as_deref().ok_or_else(|| ConnectorError::invalid_field("connection_id", "Choose a connection"))
    }

    /// A string value from the connection config.
    pub fn config_str(&self, key: &str) -> Result<String, ConnectorError> {
        self.conn()?
            .config()
            .get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| ConnectorError::invalid(format!("The connection is missing `{key}`; edit the connection")))
    }

    /// A string value from the decrypted secrets.
    pub async fn secret_str(&self, key: &str) -> Result<String, ConnectorError> {
        self.conn()?
            .secrets()
            .await?
            .get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| ConnectorError::AuthExpired { message: format!("The connection has no `{key}`; reconnect it") })
    }
}

impl std::fmt::Debug for ActionCtx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActionCtx")
            .field("org_id", &self.org_id)
            .field("connection", &self.connection.as_ref().map(|c| c.id().to_owned()))
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}
