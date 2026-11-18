//! Settings from environment variables (validated at startup; fail fast).

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
    pub oauth_clients: HashMap<String, OAuthClient>,
}

/// `WEFT_RETENTION_DAYS`: 1..=3650, default 30.
fn retention_days(value: Option<&str>) -> Result<u32, String> {
    value.map_or(Ok(30), |v| {
        v.parse()
            .ok()
            .filter(|d| (1..=3650).contains(d))
            .ok_or_else(|| "WEFT_RETENTION_DAYS must be a number of days from 1 to 3650".to_owned())
    })
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

fn var(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
}

impl Settings {
    /// Reads `WEFT_*` variables. See `README.md` for the full list.
    pub fn from_env() -> Result<Self, String> {
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
        let master_keys = match (var("WEFT_MASTER_KEYS"), var("WEFT_ACTIVE_KEY_ID")) {
            (Some(keys), Some(active)) => Some((keys, active)),
            (None, None) => None,
            _ => return Err("set both WEFT_MASTER_KEYS and WEFT_ACTIVE_KEY_ID".into()),
        };
        Ok(Self {
            database_url: var("WEFT_DATABASE_URL")
                .or_else(|| var("DATABASE_URL"))
                .unwrap_or_else(|| "mysql://weft:weft@127.0.0.1:3306/weft".into()),
            bind: var("WEFT_BIND").unwrap_or_else(|| "0.0.0.0:8080".into()),
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
            sign_in: crate::auth_settings::sign_in(&var)?,
            app_url: var("WEFT_APP_URL").unwrap_or_else(|| "http://127.0.0.1:3000".into()),
            owner_emails: crate::auth_settings::owner_emails(var("WEFT_OWNER_EMAILS")),
            mail: crate::auth_settings::mail(&var),
            mail_from: var("WEFT_MAIL_FROM").unwrap_or_else(|| "Weft <no-reply@localhost>".into()),
            worker_concurrency: var("WEFT_WORKER_CONCURRENCY")
                .and_then(|v| v.parse().ok())
                .unwrap_or(16),
            oauth_clients,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retention_and_script_memory_are_range_checked() {
        assert_eq!(retention_days(None), Ok(30));
        assert_eq!(retention_days(Some("7")), Ok(7));
        assert!(retention_days(Some("0")).is_err());
        assert!(retention_days(Some("3651")).is_err());
        assert_eq!(script_memory_mb(None), Ok(512));
        assert_eq!(script_memory_mb(Some("64")), Ok(64));
        assert!(
            script_memory_mb(Some("63")).is_err(),
            "too small to run a script"
        );
        assert!(script_memory_mb(Some("lots")).is_err());
    }
}
