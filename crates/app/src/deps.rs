//! `Deps`: every port as `Arc<dyn …>`, plus configuration. Built once in `weft-server`.

use std::{collections::HashMap, sync::Arc};

use tokio::sync::Notify;
use weft_connector_sdk::{ConnectorRegistry, HttpSender, Mailer, ScriptRunner, oauth::OAuthClient};
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
    /// Days finished runs are kept when an org has no setting of its own.
    pub default_retention_days: u32,
    /// OAuth apps per connector key.
    pub oauth_clients: HashMap<String, OAuthClient>,
    /// Where people open Weft (links in emails), e.g. `https://flow.example.com`.
    pub app_url: String,
    /// The web client's origins: the only places an OAuth sign-in may return to.
    pub client_origins: Vec<String>,
    /// Verified emails that become owners of the default org when they sign in (lower-cased).
    pub owner_emails: Vec<String>,
    /// The sign-in settings the web client needs; `None` when sign-in is turned off.
    pub sign_in: Option<SignInConfig>,
    /// Who may create organisations (`WEFT_ORG_CREATION`, staff by default).
    pub org_creation: crate::tenancy::staff::OrgCreation,
    /// Who gets staff alerts by email (`WEFT_ALERT_EMAILS`; empty = logged only).
    pub alert_emails: Vec<String>,
}

/// What the web client needs to sign in with Logto (public values only).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SignInConfig {
    /// The Logto tenant, e.g. `https://tenant.logto.app`.
    pub endpoint: String,
    /// The single-page app's ID in Logto.
    pub app_id: String,
    /// The API resource to request tokens for (JWTs); `None` = opaque tokens, checked by
    /// introspection.
    pub resource: Option<String>,
    pub scopes: Vec<String>,
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
    /// Checks sign-in tokens; `None` turns sign-in off (local development and tests).
    pub identity: Option<Arc<dyn crate::ports::IdentityVerifier>>,
    /// Runs custom functions off the async threads (`core.script`).
    pub scripts: Option<Arc<dyn ScriptRunner>>,
    pub connectors: Arc<ConnectorRegistry>,
    pub config: Arc<AppConfig>,
    pub plans: Arc<PlanCache>,
    /// Wakes the worker after an enqueue (in-process `all` mode); harmless otherwise.
    pub wake: Arc<Notify>,
    /// Per-connection request limiters (in-process buckets and concurrency permits).
    pub limiters: Arc<crate::limits::ConnectionLimiters>,
    /// Serialises OAuth token refreshes per connection within this process.
    pub refresh_locks: Arc<crate::connections::handle::RefreshLocks>,
    /// How many servers share the database (refreshed by heartbeats; per-connection concurrency is shared).
    pub instances: Arc<crate::instances::LiveInstances>,
}

impl std::fmt::Debug for Deps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Deps")
            .field("config", &self.config)
            .field("connectors", &self.connectors)
            .finish_non_exhaustive()
    }
}
