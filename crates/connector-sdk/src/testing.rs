//! Test helpers for connector tests (feature `testing`).

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use serde_json::Value;

use crate::{
    ActionCtx, ConnectionHandle, ConnectorError, EmailMessage, HttpSender, Mailer,
};

/// An in-memory connection: fixed config and secrets; `force_refresh` returns `refreshed-token`.
#[derive(Debug)]
pub struct StaticConnection {
    pub id: String,
    pub config: Value,
    pub secrets: Mutex<Value>,
    pub refreshes: AtomicUsize,
}

impl StaticConnection {
    #[must_use]
    pub fn new(config: Value, secrets: Value) -> Arc<Self> {
        Arc::new(Self { id: "cxn_test".into(), config, secrets: Mutex::new(secrets), refreshes: AtomicUsize::new(0) })
    }
}

#[async_trait]
impl ConnectionHandle for StaticConnection {
    fn id(&self) -> &str {
        &self.id
    }

    fn config(&self) -> &Value {
        &self.config
    }

    async fn secrets(&self) -> Result<Value, ConnectorError> {
        Ok(self.secrets.lock().map_or(Value::Null, |s| s.clone()))
    }

    async fn access_token(&self) -> Result<String, ConnectorError> {
        let secrets = self.secrets().await?;
        secrets
            .get("access_token")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| ConnectorError::AuthExpired { message: "no access token".into() })
    }

    async fn force_refresh(&self) -> Result<String, ConnectorError> {
        self.refreshes.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut s) = self.secrets.lock()
            && let Some(map) = s.as_object_mut()
        {
            map.insert("access_token".into(), Value::String("refreshed-token".into()));
        }
        Ok("refreshed-token".into())
    }
}

/// Captures sent emails.
#[derive(Debug, Default)]
pub struct CapturingMailer {
    pub sent: Mutex<Vec<EmailMessage>>,
}

#[async_trait]
impl Mailer for CapturingMailer {
    async fn send(&self, message: &EmailMessage) -> Result<(), ConnectorError> {
        if let Ok(mut sent) = self.sent.lock() {
            sent.push(message.clone());
        }
        Ok(())
    }
}

/// Fixed test time: 2026-01-01T00:00:00Z.
#[must_use]
pub fn test_now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).single().unwrap_or_default()
}

/// A context with the given HTTP sender and optional connection.
#[must_use]
pub fn ctx(http: Arc<dyn HttpSender>, connection: Option<Arc<dyn ConnectionHandle>>) -> ActionCtx {
    ActionCtx {
        org_id: "org_test".into(),
        http,
        connection,
        idempotency_key: "idem-test".into(),
        mailer: None,
        timeout: Duration::from_secs(10),
        now: test_now(),
    }
}
