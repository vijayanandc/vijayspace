//! Settings from `config.properties` and environment variables (the environment wins; see
//! `properties.rs`), validated at startup: a bad value stops the server.

use std::collections::HashMap;

use weft_connector_sdk::oauth::OAuthClient;

/// All server settings.
#[derive(Debug, Clone)]
pub struct Settings {
    pub database_url: String,
    pub bind: String,
    pub public_url: String,
    pub client_origins: Vec<String>,
    pub master_keys: Option<(String, String)>,
    pub allow_private_networks: bool,
    /// Egress proxy for outbound app calls (`WEFT_HTTP_PROXY`); environment proxies are ignored.
    pub http_proxy: Option<String>,
    /// Days finished runs are kept unless an org sets its own (`WEFT_RETENTION_DAYS`, default 30).
    pub retention_days: u32,
    /// Run custom functions in child processes (`WEFT_SCRIPT_ISOLATION=process`, the default)
    /// or on threads of this process (`thread`, for development only).
    pub script_processes: bool,
    /// Memory a script process may use (`WEFT_SCRIPT_MEMORY_MB`, 64..=65536, default 512); its
    /// address-space cap adds the script stack.
    pub script_memory_mb: u64,
    /// Logto sign-in; `None` = sign-in turned off (`WEFT_AUTH=off`, or no Logto settings).
    pub sign_in: Option<crate::auth_settings::SignInSettings>,
    /// Where people open Weft (`WEFT_APP_URL`; links in emails).
    pub app_url: String,
    /// `WEFT_OWNER_EMAILS`: verified emails that become owners of the default org.
    pub owner_emails: Vec<String>,
    pub mail: crate::auth_settings::MailSettings,
    pub mail_from: String,
    pub worker_concurrency: usize,
    /// Database connections for this process (`WEFT_DB_POOL_SIZE`).
    pub db_pool_size: u32,
    pub oauth_clients: HashMap<String, OAuthClient>,
    /// Connector settings (`WEFT_ZOHO_ACCOUNTS_URL`).
    pub connector_options: weft_connectors::ConnectorOptions,
    /// `WEFT_ORG_CREATION`: `staff` (default) or `anyone`.
    pub org_creation: weft_app::tenancy::staff::OrgCreation,
    /// `WEFT_ALERT_EMAILS`: staff who get alert emails (comma-separated).
    pub alert_emails: Vec<String>,
}

/// `WEFT_RETENTION_DAYS`: 1..=30 (the platform cap), default 30.
fn retention_days(value: Option<&str>) -> Result<u32, String> {
    value.map_or(Ok(30), |v| {
        v.parse()
            .ok()
            .filter(|d| (1..=weft_app::retention::MAX_DAYS).contains(d))
            .ok_or_else(|| "WEFT_RETENTION_DAYS must be a number of days from 1 to 30".to_owned())
    })
}

/// `WEFT_ZOHO_ACCOUNTS_URL`: a Zoho accounts server to use instead of Zoho's (testing against a mock). It
/// receives the OAuth client secret, so it must be HTTPS unless it is on this machine.
fn zoho_accounts_url(value: Option<String>) -> Result<Option<String>, String> {
    let Some(raw) = value else {
        return Ok(None);
    };
    let authority = |rest: &str| rest.split('/').next().unwrap_or_default().to_owned();
    let ok = match (raw.strip_prefix("https://"), raw.strip_prefix("http://")) {
        (Some(rest), _) => !authority(rest).is_empty() && !authority(rest).contains('@'),
        (None, Some(rest)) => {
            !authority(rest).contains('@') && crate::auth_settings::is_loopback(&authority(rest))
        }
        (None, None) => false,
    };
    if !ok {
        return Err(format!(
            "WEFT_ZOHO_ACCOUNTS_URL must be an https URL, or http on this machine: `{raw}`"
        ));
    }
    tracing::warn!(
        url = %raw,
        "Zoho sign-in and tokens use WEFT_ZOHO_ACCOUNTS_URL instead of Zoho (testing only)"
    );
    Ok(Some(raw.trim_end_matches('/').to_owned()))
}

/// `WEFT_SCRIPT_MEMORY_MB`: 64..=65536, default 512.
fn script_memory_mb(value: Option<&str>) -> Result<u64, String> {
    value.map_or(Ok(crate::script_child::DEFAULT_MEMORY_MB), |v| {
        v.parse()
            .ok()
            .filter(|mb| (64..=65_536).contains(mb))
            .ok_or_else(|| {
                "WEFT_SCRIPT_MEMORY_MB must be a number of MB from 64 to 65536".to_owned()
            })
    })
}

impl Settings {
    /// Reads the settings from `config.properties` and the environment. Logs file keys no setting reads.
    pub fn load(source: &crate::properties::Source, serves_api: bool) -> Result<Self, String> {
        let asked = std::cell::RefCell::new(Vec::new());
        let var = |name: &str| {
            asked.borrow_mut().push(name.to_owned());
            source.get(name)
        };
        let settings = Self::from_source(&var, serves_api)?;
        if let Some(path) = source.path() {
            let mut asked = asked.borrow().clone();
            asked.extend(crate::auth_settings::SIGN_IN_KEYS.map(str::to_owned));
            asked.extend(["WEFT_LOG", "WEFT_LOG_FORMAT", "WEFT_CONFIG_PATH"].map(str::to_owned));
            let unknown = source.unknown_keys(&asked);
            if !unknown.is_empty() {
                tracing::warn!(path, ?unknown, "config keys no setting reads (typos?)");
            }
        }
        Ok(settings)
    }

    /// Reads `WEFT_*` settings through `var` (see `README.md` for the full list). `serves_api`: whether this
    /// process answers API requests (only then are the sign-in settings required).
    pub fn from_source(
        var: &dyn Fn(&str) -> Option<String>,
        serves_api: bool,
    ) -> Result<Self, String> {
        let public_url = var("WEFT_PUBLIC_URL").unwrap_or_else(|| "http://localhost:8080".into());
        let mut oauth_clients = HashMap::new();
        for connector in ["zoho_crm", "autodesk_forma"] {
            let prefix = format!("WEFT_OAUTH_{}", connector.to_uppercase());
            if let (Some(id), Some(secret)) = (
                var(&format!("{prefix}_CLIENT_ID")),
                var(&format!("{prefix}_CLIENT_SECRET")),
            ) {
                oauth_clients.insert(
                    connector.to_owned(),
                    OAuthClient {
                        client_id: id,
                        client_secret: secret,
                        redirect_uri: format!(
                            "{}/oauth/callback",
                            public_url.trim_end_matches('/')
                        ),
                    },
                );
            }
        }
        let bind = var("WEFT_BIND").unwrap_or_else(|| "0.0.0.0:8080".into());
        let master_keys = master_keys(var("WEFT_MASTER_KEYS"), var("WEFT_ACTIVE_KEY_ID"), &bind)?;
        let worker_concurrency = number(var("WEFT_WORKER_CONCURRENCY").as_deref(), 16, 1, 256)
            .map_err(|e| format!("WEFT_WORKER_CONCURRENCY {e}"))?;
        // The worker holds a connection per running job (heartbeats, checkpoints) while the API serves requests.
        let workers = u32::try_from(worker_concurrency).unwrap_or(256);
        let db_pool_size = number(
            var("WEFT_DB_POOL_SIZE").as_deref(),
            workers + 16,
            workers + 4,
            1000,
        )
        .map_err(|e| format!("WEFT_DB_POOL_SIZE {e} (at least WEFT_WORKER_CONCURRENCY + 4)"))?;
        Ok(Self {
            database_url: var("WEFT_DATABASE_URL")
                .or_else(|| var("DATABASE_URL"))
                .unwrap_or_else(|| "mysql://weft:weft@127.0.0.1:3306/weft".into()),
            bind: bind.clone(),
            client_origins: var("WEFT_CLIENT_ORIGINS")
                .unwrap_or_else(|| "http://127.0.0.1:3000,http://localhost:5173".into())
                .split(',')
                .map(|s| s.trim().to_owned())
                .collect(),
            public_url,
            master_keys,
            script_processes: var("WEFT_SCRIPT_ISOLATION").as_deref() != Some("thread"),
            script_memory_mb: script_memory_mb(var("WEFT_SCRIPT_MEMORY_MB").as_deref())?,
            retention_days: retention_days(var("WEFT_RETENTION_DAYS").as_deref())?,
            http_proxy: var("WEFT_HTTP_PROXY"),
            allow_private_networks: var("WEFT_ALLOW_PRIVATE_NETWORKS")
                .is_some_and(|v| v == "true" || v == "1"),
            sign_in: if serves_api {
                crate::auth_settings::sign_in(var, &bind)?
            } else {
                None
            },
            app_url: var("WEFT_APP_URL").unwrap_or_else(|| "http://127.0.0.1:3000".into()),
            owner_emails: crate::auth_settings::owner_emails(var("WEFT_OWNER_EMAILS")),
            mail: crate::auth_settings::mail(var),
            mail_from: var("WEFT_MAIL_FROM").unwrap_or_else(|| "Weft <no-reply@localhost>".into()),
            worker_concurrency,
            db_pool_size,
            oauth_clients,
            connector_options: weft_connectors::ConnectorOptions {
                zoho_accounts_url: zoho_accounts_url(var("WEFT_ZOHO_ACCOUNTS_URL"))?,
            },
            alert_emails: var("WEFT_ALERT_EMAILS")
                .map(|v| {
                    v.split(',')
                        .map(|e| e.trim().to_owned())
                        .filter(|e| !e.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
            org_creation: match var("WEFT_ORG_CREATION").as_deref() {
                None | Some("staff") => weft_app::tenancy::staff::OrgCreation::Staff,
                Some("anyone") => weft_app::tenancy::staff::OrgCreation::Anyone,
                Some(other) => {
                    return Err(format!(
                        "WEFT_ORG_CREATION must be `staff` or `anyone`, not `{other}`"
                    ));
                }
            },
        })
    }
}

/// `WEFT_MASTER_KEYS` with `WEFT_ACTIVE_KEY_ID`. Without them connection secrets would be encrypted with the
/// development key, which anyone can derive from the source: allowed only on a loopback bind (local development).
fn master_keys(
    keys: Option<String>,
    active: Option<String>,
    bind: &str,
) -> Result<Option<(String, String)>, String> {
    match (keys, active) {
        (Some(keys), Some(active)) => Ok(Some((keys, active))),
        (None, None) if crate::auth_settings::is_loopback(bind) => Ok(None),
        (None, None) => Err(format!(
            "set WEFT_MASTER_KEYS and WEFT_ACTIVE_KEY_ID: the development key is only for a loopback WEFT_BIND, not `{bind}`"
        )),
        _ => Err("set both WEFT_MASTER_KEYS and WEFT_ACTIVE_KEY_ID".into()),
    }
}

#[cfg(test)]
#[test]
fn a_zoho_accounts_override_is_https_or_on_this_machine() {
    let ok = |v: &str| zoho_accounts_url(Some(v.to_owned()));
    assert_eq!(
        ok("http://127.0.0.1:9200/").unwrap().as_deref(),
        Some("http://127.0.0.1:9200")
    );
    assert!(ok("https://accounts.zoho.example").is_ok());
    assert!(
        ok("http://zoho.example.com").is_err(),
        "the client secret never goes out in clear text"
    );
    assert!(ok("http://user@127.0.0.1:9200").is_err());
    assert!(ok("ftp://127.0.0.1").is_err());
    assert_eq!(zoho_accounts_url(None).unwrap(), None);
}

/// A whole number in `min..=max`, or `default` when unset.
fn number<T>(value: Option<&str>, default: T, min: T, max: T) -> Result<T, String>
where
    T: std::str::FromStr + PartialOrd + std::fmt::Display + Copy,
{
    let Some(raw) = value else {
        return Ok(default);
    };
    match raw.trim().parse::<T>() {
        Ok(n) if n >= min && n <= max => Ok(n),
        _ => Err(format!(
            "must be a whole number from {min} to {max}, not `{raw}`"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_needs_master_keys_and_numbers_are_checked() {
        assert_eq!(master_keys(None, None, "127.0.0.1:8080"), Ok(None));
        assert!(master_keys(None, None, "0.0.0.0:8080").is_err());
        assert!(master_keys(Some("k".into()), None, "127.0.0.1:8080").is_err());
        assert!(master_keys(Some("k".into()), Some("1".into()), "0.0.0.0:8080").is_ok());
        assert_eq!(number::<usize>(None, 16, 1, 256), Ok(16));
        assert_eq!(number::<usize>(Some("32"), 16, 1, 256), Ok(32));
        assert!(
            number::<usize>(Some("lots"), 16, 1, 256).is_err(),
            "no silent default"
        );
        assert!(number::<u32>(Some("8"), 36, 20, 1000).is_err());
    }

    #[test]
    fn retention_and_script_memory_are_range_checked() {
        assert_eq!(retention_days(None), Ok(30));
        assert_eq!(retention_days(Some("7")), Ok(7));
        assert!(retention_days(Some("0")).is_err());
        assert!(retention_days(Some("30")).is_ok());
        assert!(
            retention_days(Some("31")).is_err(),
            "history is kept at most 30 days"
        );
        assert_eq!(script_memory_mb(None), Ok(512));
        assert_eq!(script_memory_mb(Some("64")), Ok(64));
        assert!(
            script_memory_mb(Some("63")).is_err(),
            "too small to run a script"
        );
        assert!(script_memory_mb(Some("lots")).is_err());
    }
}
