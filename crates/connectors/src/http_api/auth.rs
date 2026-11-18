//! Signing requests of an "Any app" connection: the URL stays under the base URL, and the credentials go where the
//! connection says (a header, the query string, a token with any header name and prefix, Basic auth, or an OAuth
//! 2.0 client-credentials token, fetched from the app and cached until it expires). The connection can also add
//! headers (plain or secret) and query parameters to every call.

use std::{collections::HashMap, sync::Mutex};

use base64::Engine as _;
use chrono::{DateTime, Duration, Utc};
use weft_connector_sdk::{ClientAuth, OAuth2Spec, prelude::*};

use crate::util::{key_values, opt_str};

/// A client-credentials token is fetched again this long before it expires.
const EARLY_SECS: i64 = 60;

/// Cached client-credentials tokens, per connection.
#[derive(Debug, Default)]
pub(super) struct TokenCache {
    tokens: Mutex<HashMap<String, (String, DateTime<Utc>)>>,
}

impl TokenCache {
    fn get(&self, connection: &str, now: DateTime<Utc>) -> Option<String> {
        let tokens = self.tokens.lock().ok()?;
        tokens
            .get(connection)
            .filter(|(_, until)| *until > now)
            .map(|(t, _)| t.clone())
    }

    fn put(&self, connection: &str, token: &str, until: DateTime<Utc>) {
        if let Ok(mut tokens) = self.tokens.lock() {
            tokens.insert(connection.to_owned(), (token.to_owned(), until));
        }
    }

    /// Forgets a connection's token (after the app refused it).
    pub(super) fn forget(&self, connection: &str) {
        if let Ok(mut tokens) = self.tokens.lock() {
            tokens.remove(connection);
        }
    }
}

/// `base` + `path`, refusing anything that would leave the base URL's scheme, host and port (an absolute URL or
/// `//host` in the path would otherwise send the connection's credentials elsewhere).
pub(super) fn url_for(base: &str, path: &str) -> Result<String, ConnectorError> {
    let base_url = url::Url::parse(base.trim()).map_err(|_| {
        ConnectorError::invalid_field("base_url", "The base URL is not a valid URL")
    })?;
    if !matches!(base_url.scheme(), "http" | "https") {
        return Err(ConnectorError::invalid_field(
            "base_url",
            "The base URL must start with https:// or http://",
        ));
    }
    let path = path.trim();
    if path.contains("://") || path.starts_with("//") || path.contains('\\') {
        return Err(ConnectorError::invalid_field(
            "path",
            "Use a path under the connection's base URL (e.g. /orders), not a full address",
        ));
    }
    let joined = format!(
        "{}/{}",
        base.trim().trim_end_matches('/'),
        path.trim_start_matches('/')
    );
    let url = url::Url::parse(&joined)
        .map_err(|_| ConnectorError::invalid_field("path", "The path does not make a valid URL"))?;
    if url.scheme() != base_url.scheme()
        || url.host_str() != base_url.host_str()
        || url.port_or_known_default() != base_url.port_or_known_default()
    {
        return Err(ConnectorError::invalid_field(
            "path",
            "The path must stay on the connection's host",
        ));
    }
    Ok(joined.trim_end_matches('/').to_owned())
}

/// Headers no connection or step may set: they frame the request itself (the host it goes to, its length and
/// encoding, proxy credentials), which the HTTP client sets. Setting them could smuggle a second request.
const RESERVED: [&str; 8] = [
    "host",
    "content-length",
    "transfer-encoding",
    "connection",
    "upgrade",
    "te",
    "trailer",
    "keep-alive",
];

/// Checks a custom header: a valid name that is not reserved, and a value on one line.
pub(super) fn check_header(name: &str, value: &str) -> Result<(), ConnectorError> {
    let lower = name.trim().to_ascii_lowercase();
    let valid = !lower.is_empty()
        && lower
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b));
    if !valid || RESERVED.contains(&lower.as_str()) || lower.starts_with("proxy-") {
        return Err(ConnectorError::invalid_field(
            "headers",
            format!("`{}` cannot be set as a header", name.trim()),
        ));
    }
    if value.contains(['\r', '\n']) {
        return Err(ConnectorError::invalid_field(
            "headers",
            format!("The value of `{}` must be on one line", name.trim()),
        ));
    }
    Ok(())
}

/// Where a token goes: the `token_header` (default `Authorization`) as `{token_prefix} {token}`. The prefix defaults
/// to `Bearer`; Zoho's APIs want `Zoho-oauthtoken`, and `none` sends the token alone.
pub(super) fn token_header(
    config: &Value,
    token: &str,
) -> Result<(String, String), ConnectorError> {
    let name = opt_str(config, "token_header").unwrap_or_else(|| "Authorization".into());
    let prefix = opt_str(config, "token_prefix").unwrap_or_else(|| "Bearer".into());
    let value = if prefix.eq_ignore_ascii_case("none") {
        token.to_owned()
    } else {
        format!("{} {token}", prefix.trim())
    };
    check_header(&name, &value).map_err(|_| {
        ConnectorError::invalid_field("token_header", "The token header or prefix is not valid")
    })?;
    Ok((name.trim().to_owned(), value))
}

/// The header the connection's sign-in sets, if any (custom headers of the same name give way to it).
fn auth_header_name(config: &Value, sign_in: bool) -> Option<String> {
    let token_name = || opt_str(config, "token_header").unwrap_or_else(|| "Authorization".into());
    if sign_in {
        return Some(token_name());
    }
    match opt_str(config, "auth").as_deref().unwrap_or("none") {
        "bearer" | "oauth2_client_credentials" => Some(token_name()),
        "api_key_header" => {
            Some(opt_str(config, "header_name").unwrap_or_else(|| "X-API-Key".into()))
        }
        "basic" => Some("Authorization".into()),
        _ => None,
    }
}

/// Adds the connection's headers, query parameters and credentials to `req`, in this order: the connection's
/// headers on every call, then its secret headers, then the sign-in. A header the connection sets replaces one of
/// the same name from the step. `sign_in`: the connection is an OAuth 2.0 sign-in at the app, whose token the
/// platform keeps fresh (`fresh`: a token just refreshed after a 401).
pub(super) async fn sign(
    ctx: &ActionCtx,
    cache: &TokenCache,
    mut req: HttpRequest,
    sign_in: bool,
    fresh: Option<&str>,
) -> Result<HttpRequest, ConnectorError> {
    let conn = ctx.conn()?;
    let config = conn.config().clone();
    let secrets = conn.secrets().await?;
    let auth_name = auth_header_name(&config, sign_in);
    let mut headers = key_values(&config, "default_headers");
    headers.extend(key_values(&secrets, "secret_headers"));
    headers.retain(|(k, _)| {
        auth_name
            .as_ref()
            .is_none_or(|a| !a.eq_ignore_ascii_case(k))
    });
    for (k, v) in &headers {
        check_header(k, v)?;
    }
    let mut set_by_connection: Vec<String> = headers
        .iter()
        .map(|(k, _)| k.to_ascii_lowercase())
        .collect();
    set_by_connection.extend(auth_name.iter().map(|a| a.to_ascii_lowercase()));
    req.headers
        .retain(|(k, _)| !set_by_connection.contains(&k.to_ascii_lowercase()));
    for (k, v) in headers {
        req = req.header(&k, v);
    }
    let query = key_values(&config, "default_query");
    let refs: Vec<(&str, &str)> = query
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    req = req.query(&refs);
    if sign_in {
        let token = match fresh {
            Some(t) => t.to_owned(),
            None => conn.access_token().await?,
        };
        let (name, value) = token_header(&config, &token)?;
        return Ok(req.header(&name, value));
    }
    let secret = |key: &str| {
        opt_str(&secrets, key).ok_or_else(|| ConnectorError::AuthExpired {
            message: format!("the connection has no {key}; edit it and enter it again"),
        })
    };
    Ok(
        match opt_str(&config, "auth").as_deref().unwrap_or("none") {
            "api_key_header" => {
                let name = opt_str(&config, "header_name").unwrap_or_else(|| "X-API-Key".into());
                check_header(&name, "")?;
                req.header(&name, secret("api_key")?)
            }
            "api_key_query" => {
                let name = opt_str(&config, "query_name").unwrap_or_else(|| "api_key".into());
                req.secret_query(&name, &secret("api_key")?)
            }
            "bearer" => {
                let (name, value) = token_header(&config, &secret("token")?)?;
                req.header(&name, value)
            }
            "basic" => {
                let user = opt_str(&config, "username").unwrap_or_default();
                let pair = format!("{user}:{}", secret("password")?);
                req.header(
                    "Authorization",
                    format!(
                        "Basic {}",
                        base64::engine::general_purpose::STANDARD.encode(pair)
                    ),
                )
            }
            "oauth2_client_credentials" => {
                let token = client_token(ctx, cache, &config, &secrets).await?;
                let (name, value) = token_header(&config, &token)?;
                req.header(&name, value)
            }
            _ => req,
        },
    )
}

/// A client-credentials access token: the cached one, else a new one from the token URL.
async fn client_token(
    ctx: &ActionCtx,
    cache: &TokenCache,
    config: &Value,
    secrets: &Value,
) -> Result<String, ConnectorError> {
    let id = ctx.conn()?.id().to_owned();
    if let Some(token) = cache.get(&id, ctx.now) {
        return Ok(token);
    }
    let token_url = opt_str(config, "token_url")
        .ok_or_else(|| ConnectorError::invalid_field("token_url", "Enter the token URL"))?;
    let client_id = opt_str(config, "client_id").unwrap_or_default();
    let client_secret = opt_str(secrets, "client_secret").unwrap_or_default();
    let scope = opt_str(config, "scope").unwrap_or_default();
    let mut form = vec![
        ("grant_type", "client_credentials"),
        ("client_id", client_id.as_str()),
        ("client_secret", client_secret.as_str()),
    ];
    if !scope.is_empty() {
        form.push(("scope", scope.as_str()));
    }
    let resp = ctx
        .http
        .send(
            HttpRequest::new(Method::Post, token_url)
                .form(&form)
                .timeout(ctx.timeout),
        )
        .await?;
    if !resp.is_success() {
        return Err(ConnectorError::AuthExpired {
            message: format!(
                "the app refused the client credentials (HTTP {})",
                resp.status
            ),
        });
    }
    let body = resp.json()?;
    let token = opt_str(&body, "access_token").ok_or_else(|| ConnectorError::AuthExpired {
        message: "the token answer has no access_token".into(),
    })?;
    let lifetime = body
        .get("expires_in")
        .and_then(Value::as_i64)
        .unwrap_or(3600);
    cache.put(
        &id,
        &token,
        ctx.now + Duration::seconds((lifetime - EARLY_SECS).max(0)),
    );
    Ok(token)
}

/// The OAuth 2.0 settings of an "Any app (OAuth 2.0 sign-in)" connection, from its config. The authorize and
/// token URLs must be https (or http on this machine, for a local test double of the app).
pub(super) fn sign_in_spec(config: &Value) -> Result<OAuth2Spec, ConnectorError> {
    let url = |key: &str, label: &str| {
        let raw = opt_str(config, key)
            .ok_or_else(|| ConnectorError::invalid_field(key, format!("Enter the {label}")))?;
        let parsed = url::Url::parse(&raw).map_err(|_| {
            ConnectorError::invalid_field(key, format!("The {label} is not a valid URL"))
        })?;
        let local = matches!(parsed.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"));
        if parsed.scheme() == "https" || (parsed.scheme() == "http" && local) {
            Ok(raw)
        } else {
            Err(ConnectorError::invalid_field(
                key,
                format!("The {label} must start with https://"),
            ))
        }
    };
    let scopes = opt_str(config, "scopes")
        .unwrap_or_default()
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    Ok(OAuth2Spec {
        authorize_url: url("authorize_url", "authorization URL")?,
        token_url: url("token_url", "token URL")?,
        scopes,
        scope_separator: " ".into(),
        extra_authorize_params: key_values(config, "authorize_params"),
        pkce: crate::util::opt_bool(config, "pkce").unwrap_or(true),
        client_auth: if opt_str(config, "client_auth").as_deref() == Some("basic") {
            ClientAuth::Basic
        } else {
            ClientAuth::Body
        },
    })
}
