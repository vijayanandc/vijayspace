//! The `Connector` trait.

use std::collections::BTreeMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    context::ActionCtx,
    error::ConnectorError,
    manifest::{ConnectorManifest, FieldSpec, OptionItem},
    records::RecordObjects,
};

/// One new item from a trigger (polled or pushed).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventItem {
    /// Unique per item *version*: the platform runs each dedupe key at most once per flow.
    pub dedupe_key: String,
    /// Record ID, used for debouncing and links (optional).
    #[serde(default)]
    pub record_id: Option<String>,
    /// Becomes `trigger` in templates.
    pub data: Value,
}

/// Result of one poll.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PollResult {
    pub items: Vec<EventItem>,
    /// Cursor to pass to the next poll (`None` keeps the previous one).
    pub cursor: Option<Value>,
}

/// Parameters for registering a webhook at the app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscribeRequest {
    /// Where the app must send events.
    pub target_url: String,
    /// A secret the app echoes back (Zoho `token`) or that Weft checks in a header.
    pub verify_token: String,
    /// A numeric channel ID for providers that need one (Zoho `channel_id`).
    pub channel_id: u64,
}

/// A registered subscription at the app.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubscriptionInfo {
    pub external_id: String,
    /// RFC 3339; the platform renews before this.
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub data: Value,
}

/// An inbound HTTP request delivered to a trigger.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InboundRequest {
    pub method: String,
    /// Lower-cased header names.
    pub headers: BTreeMap<String, String>,
    pub query: BTreeMap<String, String>,
    /// Parsed body (JSON, form → object, text → string).
    pub body: Value,
}

impl InboundRequest {
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }
}

/// Items extracted from one inbound request, plus an optional immediate response body
/// (for handshakes such as challenge echoes).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventBatch {
    pub items: Vec<EventItem>,
    #[serde(default)]
    pub response: Option<Value>,
}

/// A connector. Only `manifest` and `execute` are mandatory; the rest default to "unsupported".
#[async_trait]
pub trait Connector: Send + Sync {
    fn manifest(&self) -> &ConnectorManifest;

    /// Runs one action with rendered input; returns the step output.
    async fn execute(
        &self,
        ctx: &ActionCtx,
        action: &str,
        input: Value,
    ) -> Result<Value, ConnectorError>;

    /// Checks the credentials; returns display info (account name, org) stored on the connection.
    async fn test_connection(&self, _ctx: &ActionCtx) -> Result<Value, ConnectorError> {
        Ok(Value::Null)
    }

    /// Dynamic dropdown options.
    async fn options(
        &self,
        _ctx: &ActionCtx,
        source: &str,
        _input: &Value,
    ) -> Result<Vec<OptionItem>, ConnectorError> {
        Err(ConnectorError::unsupported(format!("options `{source}`")))
    }

    /// Dynamic sub-fields (e.g. the fields of the chosen module).
    async fn fields(
        &self,
        _ctx: &ActionCtx,
        source: &str,
        _input: &Value,
    ) -> Result<Vec<FieldSpec>, ConnectorError> {
        Err(ConnectorError::unsupported(format!("fields `{source}`")))
    }

    /// Polling triggers and reconciliation.
    async fn poll(
        &self,
        _ctx: &ActionCtx,
        trigger: &str,
        _input: &Value,
        _cursor: Option<&Value>,
    ) -> Result<PollResult, ConnectorError> {
        Err(ConnectorError::unsupported(format!(
            "polling for `{trigger}`"
        )))
    }

    /// Registers a webhook at the app (instant triggers that support it).
    async fn subscribe(
        &self,
        _ctx: &ActionCtx,
        trigger: &str,
        _input: &Value,
        _req: &SubscribeRequest,
    ) -> Result<SubscriptionInfo, ConnectorError> {
        Err(ConnectorError::unsupported(format!(
            "subscriptions for `{trigger}`"
        )))
    }

    /// Extends a subscription before it expires.
    async fn renew(
        &self,
        ctx: &ActionCtx,
        trigger: &str,
        input: &Value,
        _sub: &SubscriptionInfo,
        req: &SubscribeRequest,
    ) -> Result<SubscriptionInfo, ConnectorError> {
        self.subscribe(ctx, trigger, input, req).await
    }

    /// Removes a subscription (best effort).
    async fn unsubscribe(
        &self,
        _ctx: &ActionCtx,
        _trigger: &str,
        _sub: &SubscriptionInfo,
    ) -> Result<(), ConnectorError> {
        Ok(())
    }

    /// Verifies and parses an inbound event; may fetch full records through `ctx`.
    async fn handle_event(
        &self,
        _ctx: &ActionCtx,
        trigger: &str,
        _input: &Value,
        _request: &InboundRequest,
        _verify_token: &str,
    ) -> Result<EventBatch, ConnectorError> {
        Err(ConnectorError::unsupported(format!(
            "events for `{trigger}`"
        )))
    }

    /// Checks an action's input when a flow is validated (draft save and publish), as
    /// `(field, message)` pairs. Must be pure and fast: no IO.
    fn check_input(&self, _action: &str, _input: &Value) -> Vec<(String, String)> {
        Vec::new()
    }

    /// The record capability, for metadata-driven apps.
    fn records(&self) -> Option<&dyn RecordObjects> {
        None
    }

    /// Config to store from the OAuth callback's query parameters (e.g. Zoho `accounts-server`).
    fn oauth_callback_config(&self, _query: &BTreeMap<String, String>) -> Value {
        Value::Null
    }

    /// Config to store from the token response (e.g. Zoho `api_domain`).
    fn oauth_token_config(&self, _token_response: &Value) -> Value {
        Value::Null
    }
}
