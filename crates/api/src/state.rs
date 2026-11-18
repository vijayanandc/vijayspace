//! Shared handler state.

use std::sync::Arc;

use weft_app::Deps;

use crate::middleware::WebhookLimiter;

/// State passed to every handler.
#[derive(Debug, Clone)]
pub struct ApiState {
    pub deps: Deps,
    /// Per-URL rate limits of webhook ingress (this process).
    pub webhooks: Arc<WebhookLimiter>,
}

impl ApiState {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self {
            deps,
            webhooks: Arc::new(WebhookLimiter::default()),
        }
    }
}
