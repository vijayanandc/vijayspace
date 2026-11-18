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
    pub smtp_url: Option<String>,
    pub mail_from: String,
    pub worker_concurrency: usize,
    pub oauth_clients: HashMap<String, OAuthClient>,
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
                .unwrap_or_else(|| "http://localhost:5173".into())
                .split(',')
                .map(|s| s.trim().to_owned())
                .collect(),
            public_url,
            master_keys,
            retention_days: var("WEFT_RETENTION_DAYS")
                .and_then(|v| v.parse().ok())
                .unwrap_or(30),
            http_proxy: var("WEFT_HTTP_PROXY"),
            allow_private_networks: var("WEFT_ALLOW_PRIVATE_NETWORKS")
                .is_some_and(|v| v == "true" || v == "1"),
            smtp_url: var("WEFT_SMTP_URL"),
            mail_from: var("WEFT_MAIL_FROM").unwrap_or_else(|| "Weft <no-reply@localhost>".into()),
            worker_concurrency: var("WEFT_WORKER_CONCURRENCY")
                .and_then(|v| v.parse().ok())
                .unwrap_or(16),
            oauth_clients,
        })
    }
}
