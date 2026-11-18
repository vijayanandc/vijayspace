//! Builds `Deps` from settings: the only place that knows every concrete adapter.

use std::sync::Arc;

use tokio::sync::Notify;
use weft_app::{AppConfig, Deps, connections::handle::RefreshLocks, runner::plan_cache::PlanCache};
use weft_connector_sdk::{ConnectorRegistry, Mailer};
use weft_db::MySqlStore;
use weft_domain::ids::OrgId;
use weft_http::{ReqwestSender, SsrfPolicy};
use weft_infra::{
    AesGcmSecretBox, LogMailer, PooledScriptRunner, SmtpMailer, SystemClock, UuidV7Ids,
};

use crate::config::Settings;

/// The default org used while authentication is disabled (fixed so restarts keep it).
pub const DEFAULT_ORG: uuid::Uuid =
    uuid::Uuid::from_u128(0x0190_0000_0000_7000_8000_0000_0000_0001);

/// Every connector available on this server.
#[must_use]
pub fn registry() -> ConnectorRegistry {
    let mut registry = ConnectorRegistry::new();
    for c in weft_connectors::builtin_connectors() {
        registry.register(c);
    }
    registry.register(Arc::new(weft_app::sync::SyncConnector::new()));
    registry
}

/// Builds dependencies from settings and an open pool.
pub fn build(settings: &Settings, store: MySqlStore) -> anyhow::Result<Deps> {
    let secrets = if let Some((keys, active)) = &settings.master_keys {
        AesGcmSecretBox::from_config(keys, active).map_err(anyhow::Error::msg)?
    } else {
        tracing::warn!(
            "WEFT_MASTER_KEYS is not set: using the development key. Do not use this in production."
        );
        AesGcmSecretBox::development()
    };
    let policy = if settings.allow_private_networks {
        SsrfPolicy::allow_private()
    } else {
        SsrfPolicy::public_only()
    };
    let mailer: Arc<dyn Mailer> = match &settings.smtp_url {
        Some(url) => {
            Arc::new(SmtpMailer::new(url, &settings.mail_from).map_err(anyhow::Error::msg)?)
        }
        None => Arc::new(LogMailer),
    };
    Ok(Deps {
        store: Arc::new(store),
        clock: Arc::new(SystemClock),
        ids: Arc::new(UuidV7Ids),
        secrets: Arc::new(secrets),
        http: Arc::new(
            ReqwestSender::with_proxy(policy, settings.http_proxy.as_deref())
                .map_err(anyhow::Error::msg)?,
        ),
        mailer: Some(mailer),
        scripts: Some(Arc::new(PooledScriptRunner::new(
            std::thread::available_parallelism().map_or(2, std::num::NonZeroUsize::get),
            weft_script::Limits::default(),
        ))),
        connectors: Arc::new(registry()),
        config: Arc::new(AppConfig {
            public_url: settings.public_url.clone(),
            default_retention_days: settings.retention_days,
            default_org_id: OrgId::from_uuid(DEFAULT_ORG),
            oauth_clients: settings.oauth_clients.clone(),
        }),
        plans: Arc::new(PlanCache::default()),
        wake: Arc::new(Notify::new()),
        limiters: Arc::new(weft_app::limits::ConnectionLimiters::default()),
        refresh_locks: Arc::new(RefreshLocks::default()),
    })
}
