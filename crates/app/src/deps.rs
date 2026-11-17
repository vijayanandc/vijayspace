//! `Deps`: every port as `Arc<dyn …>`, plus configuration. Built once in `weft-server`.

use std::{collections::HashMap, sync::Arc};

use tokio::sync::Notify;
use weft_connector_sdk::{ConnectorRegistry, HttpSender, Mailer, oauth::OAuthClient};
use weft_domain::ids::OrgId;

use crate::{
    ports::{Clock, IdGen, SecretBox, Store},
    runner::plan_cache::PlanCache,
};

/// Static configuration used by use cases.
#[derive(Debug, Clone)]
pub struct AppConfig {
    /// Public base URL of the API (webhook and OAuth callback URLs are built from it).
    pub public_url: String,
    /// The org used when a request has no `X-Org-ID` (authentication is not enabled yet).
    pub default_org_id: OrgId,
    /// OAuth apps per connector key.
    pub oauth_clients: HashMap<String, OAuthClient>,
}

/// All dependencies of the use cases.
#[derive(Clone)]
pub struct Deps {
    pub store: Arc<dyn Store>,
    pub clock: Arc<dyn Clock>,
    pub ids: Arc<dyn IdGen>,
    pub secrets: Arc<dyn SecretBox>,
    pub http: Arc<dyn HttpSender>,
    pub mailer: Option<Arc<dyn Mailer>>,
    pub connectors: Arc<ConnectorRegistry>,
    pub config: Arc<AppConfig>,
    pub plans: Arc<PlanCache>,
    /// Wakes the worker after an enqueue (in-process `all` mode); harmless otherwise.
    pub wake: Arc<Notify>,
    /// Serialises OAuth token refreshes per connection within this process.
    pub refresh_locks: Arc<crate::connections::handle::RefreshLocks>,
}

impl std::fmt::Debug for Deps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Deps")
            .field("config", &self.config)
            .field("connectors", &self.connectors)
            .finish_non_exhaustive()
    }
}
