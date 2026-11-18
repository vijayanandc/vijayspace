//! `autodesk_forma` connector: Autodesk Forma (formerly Autodesk Construction Cloud) via
//! Autodesk Platform Services. Forma Build projects, issues, RFIs and cost webhooks.
//! See `docs/plan/13-mvp-zoho-crm-appblocks.md` §5.3 (recipes R7–R9).
//!
//! The APS endpoints kept their `/construction/...` paths after the March 2026 rebrand.
//! Shapes not yet confirmed against a live account are marked `VERIFY (spike M37a)`.

mod actions;
mod client;
mod manifest;
mod options;
mod triggers;

use weft_connector_sdk::{
    EventBatch, InboundRequest, PollResult, SubscribeRequest, SubscriptionInfo, prelude::*,
};

/// The Autodesk Forma connector.
#[derive(Debug)]
pub struct FormaConnector {
    manifest: ConnectorManifest,
}

impl FormaConnector {
    #[must_use]
    pub fn new() -> Self {
        Self {
            manifest: manifest::build(),
        }
    }
}

impl Default for FormaConnector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Connector for FormaConnector {
    /// VERIFY: APS rate limits are per endpoint (typically a few hundred per minute).
    fn default_limits(&self) -> ConnectionLimits {
        ConnectionLimits {
            requests_per_minute: Some(120),
            max_concurrent: Some(10),
            daily_requests: None,
        }
    }

    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    async fn execute(
        &self,
        ctx: &ActionCtx,
        action: &str,
        input: Value,
    ) -> Result<Value, ConnectorError> {
        actions::run(ctx, action, &input).await
    }

    async fn test_connection(&self, ctx: &ActionCtx) -> Result<Value, ConnectorError> {
        options::test_connection(ctx).await
    }

    async fn options(
        &self,
        ctx: &ActionCtx,
        source: &str,
        input: &Value,
    ) -> Result<Vec<OptionItem>, ConnectorError> {
        options::load(ctx, source, input).await
    }

    async fn poll(
        &self,
        ctx: &ActionCtx,
        trigger: &str,
        input: &Value,
        cursor: Option<&Value>,
    ) -> Result<PollResult, ConnectorError> {
        triggers::poll(ctx, trigger, input, cursor).await
    }

    async fn subscribe(
        &self,
        ctx: &ActionCtx,
        trigger: &str,
        input: &Value,
        req: &SubscribeRequest,
    ) -> Result<SubscriptionInfo, ConnectorError> {
        triggers::subscribe(ctx, trigger, input, req).await
    }

    /// APS hooks do not expire (`autoReactivateHook` keeps them alive), so renewal is a no-op.
    async fn renew(
        &self,
        _ctx: &ActionCtx,
        _trigger: &str,
        _input: &Value,
        sub: &SubscriptionInfo,
        _req: &SubscribeRequest,
    ) -> Result<SubscriptionInfo, ConnectorError> {
        Ok(sub.clone())
    }

    async fn unsubscribe(
        &self,
        ctx: &ActionCtx,
        trigger: &str,
        sub: &SubscriptionInfo,
    ) -> Result<(), ConnectorError> {
        triggers::unsubscribe(ctx, trigger, sub).await
    }

    async fn handle_event(
        &self,
        ctx: &ActionCtx,
        trigger: &str,
        input: &Value,
        request: &InboundRequest,
        verify_token: &str,
    ) -> Result<EventBatch, ConnectorError> {
        triggers::handle_event(ctx, trigger, input, request, verify_token).await
    }
}
