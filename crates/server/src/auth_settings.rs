//! Sign-in (Logto) and mail (ZeptoMail or SMTP) settings from the environment. Secrets (the
//! Logto app secret, the ZeptoMail key) are read only from the environment, never from files in
//! the repository.

/// How the server checks sign-ins.
#[derive(Clone, PartialEq, Eq)]
pub struct SignInSettings {
    pub endpoint: String,
    /// This server's confidential app (token introspection).
    pub app_id: String,
    pub app_secret: String,
    /// The web client's single-page app (sent to the client; its tokens are accepted).
    pub client_app_id: String,
    pub resource: Option<String>,
    pub scopes: Vec<String>,
    pub cache_secs: u64,
}

impl std::fmt::Debug for SignInSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignInSettings")
            .field("endpoint", &self.endpoint)
            .field("app_id", &self.app_id)
            .field("client_app_id", &self.client_app_id)
            .field("resource", &self.resource)
            .finish_non_exhaustive()
    }
}

/// Where email goes.
#[derive(Clone, PartialEq, Eq)]
pub enum MailSettings {
    ZeptoMail {
        api_key: String,
        url: String,
    },
    Smtp {
        url: String,
    },
    /// Logged only (development).
    Log,
}

impl std::fmt::Debug for MailSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeptoMail { url, .. } => write!(f, "ZeptoMail({url})"),
            Self::Smtp { .. } => f.write_str("Smtp"),
            Self::Log => f.write_str("Log"),
        }
    }
}

fn list(value: Option<String>) -> Vec<String> {
    value
        .unwrap_or_default()
        .split([',', ' '])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Whether a bind address only accepts connections from this machine.
fn is_loopback(bind: &str) -> bool {
    let host = bind.rsplit_once(':').map_or(bind, |(h, _)| h);
    let host = host.trim_start_matches('[').trim_end_matches(']');
    host == "localhost"
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

/// `WEFT_AUTH` and the `WEFT_LOGTO_*` settings it needs. Fails closed: with neither `WEFT_AUTH`
/// nor `WEFT_LOGTO_ENDPOINT` set the server does not start, and `WEFT_AUTH=off` (every request is
/// an owner) is accepted only on a loopback bind address (`WEFT_BIND=127.0.0.1:8080`).
pub fn sign_in(
    var: &dyn Fn(&str) -> Option<String>,
    bind: &str,
) -> Result<Option<SignInSettings>, String> {
    let mode = var("WEFT_AUTH").map(|m| m.to_lowercase());
    let wanted = match mode.as_deref() {
        Some("off") if is_loopback(bind) => false,
        Some("off") => {
            return Err(format!(
                "WEFT_AUTH=off lets anyone act as an owner, so it needs a loopback WEFT_BIND (e.g. 127.0.0.1:8080), not `{bind}`"
            ));
        }
        Some("logto") => true,
        None if var("WEFT_LOGTO_ENDPOINT").is_some() => true,
        None => {
            return Err(
                "set WEFT_AUTH=logto with the WEFT_LOGTO_* settings, or WEFT_AUTH=off for local development"
                    .into(),
            );
        }
        Some(other) => return Err(format!("WEFT_AUTH must be `logto` or `off`, not `{other}`")),
    };
    if !wanted {
        return Ok(None);
    }
    let need =
        |name: &str| var(name).ok_or_else(|| format!("sign-in (WEFT_AUTH=logto) needs {name}"));
    let endpoint = need("WEFT_LOGTO_ENDPOINT")?
        .trim_end_matches('/')
        .to_owned();
    if !endpoint.starts_with("https://")
        && !endpoint.starts_with("http://127.0.0.1")
        && !endpoint.starts_with("http://localhost")
    {
        return Err("WEFT_LOGTO_ENDPOINT must use https".into());
    }
    let mut scopes = list(var("WEFT_LOGTO_SCOPES"));
    if scopes.is_empty() {
        scopes = vec!["openid".into(), "profile".into(), "email".into()];
    }
    Ok(Some(SignInSettings {
        endpoint,
        app_id: need("WEFT_LOGTO_APP_ID")?,
        app_secret: need("WEFT_LOGTO_APP_SECRET")?,
        client_app_id: need("WEFT_LOGTO_CLIENT_APP_ID")?,
        resource: var("WEFT_LOGTO_RESOURCE"),
        scopes,
        cache_secs: var("WEFT_LOGTO_CACHE_SECS")
            .map(|v| {
                v.parse()
                    .map_err(|_| "WEFT_LOGTO_CACHE_SECS must be a number of seconds")
            })
            .transpose()?
            .unwrap_or(300),
    }))
}

/// ZeptoMail when `WEFT_ZEPTOMAIL_API_KEY` is set, else SMTP when `WEFT_SMTP_URL` is set, else
/// logged only.
pub fn mail(var: &dyn Fn(&str) -> Option<String>) -> MailSettings {
    if let Some(api_key) = var("WEFT_ZEPTOMAIL_API_KEY") {
        return MailSettings::ZeptoMail {
            api_key,
            url: var("WEFT_ZEPTOMAIL_API_URL")
                .unwrap_or_else(|| weft_infra::ZEPTOMAIL_API_URL.to_owned()),
        };
    }
    var("WEFT_SMTP_URL").map_or(MailSettings::Log, |url| MailSettings::Smtp { url })
}

/// Splits `Name <address>` (or a bare address) into its parts.
#[must_use]
pub fn split_from(from: &str) -> (String, String) {
    match from.rsplit_once('<') {
        Some((name, rest)) => (
            name.trim().trim_matches('"').to_owned(),
            rest.trim_end_matches('>').trim().to_owned(),
        ),
        None => (String::new(), from.trim().to_owned()),
    }
}

/// `WEFT_OWNER_EMAILS`: comma-separated, lower-cased.
#[must_use]
pub fn owner_emails(value: Option<String>) -> Vec<String> {
    list(value).into_iter().map(|e| e.to_lowercase()).collect()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |k| map.get(k).cloned()
    }

    const LOGTO: [(&str, &str); 4] = [
        ("WEFT_LOGTO_ENDPOINT", "https://tenant.logto.app/"),
        ("WEFT_LOGTO_APP_ID", "server"),
        ("WEFT_LOGTO_APP_SECRET", "s3cret"),
        ("WEFT_LOGTO_CLIENT_APP_ID", "web"),
    ];

    const BIND: &str = "0.0.0.0:8080";

    #[test]
    fn sign_in_fails_closed_and_is_off_only_on_loopback() {
        assert!(sign_in(&env(&[]), BIND).is_err(), "no silent default");
        assert!(
            sign_in(&env(&[("WEFT_AUTH", "off")]), BIND).is_err(),
            "not on a public bind"
        );
        for bind in ["127.0.0.1:8080", "localhost:8080", "[::1]:8080"] {
            assert_eq!(
                sign_in(&env(&[("WEFT_AUTH", "off")]), bind).unwrap(),
                None,
                "{bind}"
            );
        }
        assert!(sign_in(&env(&[("WEFT_AUTH", "OFF")]), "10.0.0.5:8080").is_err());
    }

    #[test]
    fn sign_in_is_on_when_logto_is_configured_and_needs_every_setting() {
        let on = sign_in(&env(&LOGTO), BIND).unwrap().unwrap();
        assert_eq!(on.endpoint, "https://tenant.logto.app");
        assert_eq!(on.scopes, ["openid", "profile", "email"]);
        assert_eq!(on.cache_secs, 300);
        assert!(
            !format!("{on:?}").contains("s3cret"),
            "the secret never reaches logs"
        );
        let mut off = LOGTO.to_vec();
        off.push(("WEFT_AUTH", "off"));
        assert_eq!(sign_in(&env(&off), "127.0.0.1:8080").unwrap(), None);
        assert!(sign_in(&env(&[("WEFT_AUTH", "logto")]), BIND).is_err());
        assert!(
            sign_in(&env(&LOGTO[..3]), BIND).is_err(),
            "the client app ID is required"
        );
        let mut plain = LOGTO.to_vec();
        plain[0] = ("WEFT_LOGTO_ENDPOINT", "http://tenant.logto.app");
        assert!(sign_in(&env(&plain), BIND).is_err(), "https only");
        assert!(sign_in(&env(&[("WEFT_AUTH", "maybe")]), BIND).is_err());
    }

    #[test]
    fn mail_prefers_zeptomail_then_smtp() {
        assert_eq!(mail(&env(&[])), MailSettings::Log);
        assert_eq!(
            mail(&env(&[("WEFT_SMTP_URL", "smtp://localhost:1025")])),
            MailSettings::Smtp {
                url: "smtp://localhost:1025".into()
            }
        );
        let zepto = mail(&env(&[
            ("WEFT_ZEPTOMAIL_API_KEY", "k"),
            ("WEFT_SMTP_URL", "smtp://x"),
        ]));
        assert!(
            matches!(zepto, MailSettings::ZeptoMail { ref url, .. } if url == weft_infra::ZEPTOMAIL_API_URL)
        );
        assert_eq!(
            split_from("Weft <noreply@appblocks.in>"),
            ("Weft".into(), "noreply@appblocks.in".into())
        );
        assert_eq!(
            split_from("noreply@appblocks.in"),
            (String::new(), "noreply@appblocks.in".into())
        );
        assert_eq!(
            owner_emails(Some("A@x.com, b@y.com".into())),
            ["a@x.com", "b@y.com"]
        );
    }
}
