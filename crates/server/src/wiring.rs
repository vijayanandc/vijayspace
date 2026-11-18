//! Builds `Deps` from settings: the only place that knows every concrete adapter.

use std::sync::Arc;

use tokio::sync::Notify;
use weft_app::{
    AppConfig, Deps, SignInConfig, connections::handle::RefreshLocks, ports::IdentityVerifier,
    runner::plan_cache::PlanCache,
};
use weft_connector_sdk::ScriptRunner;
use weft_connector_sdk::{ConnectorRegistry, Mailer};
use weft_db::MySqlStore;
use weft_domain::ids::OrgId;
use weft_http::{ReqwestSender, SsrfPolicy};
use weft_infra::{
    AesGcmSecretBox, LogMailer, LogtoSettings, LogtoVerifier, PooledScriptRunner,
    ProcessScriptRunner, SmtpMailer, SystemClock, UuidV7Ids, ZeptoMailer,
};

use crate::{
    auth_settings::{MailSettings, split_from},
    config::Settings,
};

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
    let mailer = mailer(settings)?;
    let clock: Arc<dyn weft_app::ports::Clock> = Arc::new(SystemClock);
    let identity = identity(settings, &clock)?;
    Ok(Deps {
        identity,
        store: Arc::new(store),
        clock,
        ids: Arc::new(UuidV7Ids),
        secrets: Arc::new(secrets),
        http: Arc::new(
            ReqwestSender::with_proxy(policy, settings.http_proxy.as_deref())
                .map_err(anyhow::Error::msg)?,
        ),
        mailer: Some(mailer),
        scripts: Some(script_runner(settings)?),
        connectors: Arc::new(registry()),
        config: Arc::new(AppConfig {
            public_url: settings.public_url.clone(),
            default_retention_days: settings.retention_days,
            default_org_id: OrgId::from_uuid(DEFAULT_ORG),
            oauth_clients: settings.oauth_clients.clone(),
            app_url: settings.app_url.clone(),
            client_origins: settings.client_origins.clone(),
            owner_emails: settings.owner_emails.clone(),
            sign_in: settings.sign_in.as_ref().map(|s| SignInConfig {
                endpoint: s.endpoint.clone(),
                app_id: s.client_app_id.clone(),
                resource: s.resource.clone(),
                scopes: s.scopes.clone(),
            }),
        }),
        plans: Arc::new(PlanCache::default()),
        wake: Arc::new(Notify::new()),
        limiters: Arc::new(weft_app::limits::ConnectionLimiters::default()),
        refresh_locks: Arc::new(RefreshLocks::default()),
    })
}

fn mailer(settings: &Settings) -> anyhow::Result<Arc<dyn Mailer>> {
    Ok(match &settings.mail {
        MailSettings::ZeptoMail { api_key, url } => {
            let (name, address) = split_from(&settings.mail_from);
            Arc::new(ZeptoMailer::new(url, api_key, &address, &name).map_err(anyhow::Error::msg)?)
        }
        MailSettings::Smtp { url } => {
            Arc::new(SmtpMailer::new(url, &settings.mail_from).map_err(anyhow::Error::msg)?)
        }
        MailSettings::Log => Arc::new(LogMailer),
    })
}

/// Logto sign-in, or `None` (with a warning) when it is turned off.
fn identity(
    settings: &Settings,
    clock: &Arc<dyn weft_app::ports::Clock>,
) -> anyhow::Result<Option<Arc<dyn IdentityVerifier>>> {
    let Some(s) = &settings.sign_in else {
        tracing::error!(
            "sign-in is turned off (WEFT_AUTH=off): every request acts as an owner. Development only; the server accepts connections from this machine only."
        );
        return Ok(None);
    };
    let verifier = LogtoVerifier::new(
        LogtoSettings {
            endpoint: s.endpoint.clone(),
            app_id: s.app_id.clone(),
            app_secret: s.app_secret.clone(),
            resource: s.resource.clone(),
            client_ids: vec![s.client_app_id.clone()],
            required_scopes: s.scopes.clone(),
            cache_ttl: std::time::Duration::from_secs(s.cache_secs),
            cache_size: 10_000,
        },
        Arc::clone(clock),
    )
    .map_err(anyhow::Error::msg)?;
    Ok(Some(Arc::new(verifier)))
}

/// Custom functions run in child processes under OS limits unless isolation is turned off.
fn script_runner(settings: &Settings) -> anyhow::Result<Arc<dyn ScriptRunner>> {
    let cpus = std::thread::available_parallelism().map_or(2, std::num::NonZeroUsize::get);
    if settings.script_processes {
        let exe = std::env::current_exe()?;
        return Ok(Arc::new(ProcessScriptRunner::new(
            exe,
            cpus,
            settings.script_memory_mb,
            std::time::Duration::from_secs(15),
        )));
    }
    tracing::warn!(
        "WEFT_SCRIPT_ISOLATION=thread: custom functions run inside this process (development only)"
    );
    Ok(Arc::new(PooledScriptRunner::new(
        cpus,
        weft_script::Limits::default(),
    )))
}
