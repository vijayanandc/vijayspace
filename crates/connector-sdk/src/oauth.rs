//! Generic OAuth 2.0 helpers (authorize URL, PKCE, code exchange, refresh).

use base64::Engine as _;
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use rand::RngCore as _;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{
    error::ConnectorError,
    http::{HttpRequest, HttpSender},
    manifest::{ClientAuth, OAuth2Spec},
};

/// The platform's registered OAuth app for one connector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthClient {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
}

/// Replaces `{{ config.<key> }}` placeholders with values from the connection config.
#[must_use]
pub fn fill(template: &str, config: &Value) -> String {
    let mut out = template.to_owned();
    if let Value::Object(map) = config {
        for (key, value) in map {
            if let Some(s) = value.as_str() {
                out = out.replace(&format!("{{{{ config.{key} }}}}"), s).replace(&format!("{{{{config.{key}}}}}"), s);
            }
        }
    }
    out
}

/// 32 random bytes, base64url without padding.
#[must_use]
pub fn random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// A PKCE `(verifier, S256 challenge)` pair.
#[must_use]
pub fn pkce_pair() -> (String, String) {
    let verifier = random_token();
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    (verifier, challenge)
}

/// Builds the URL the user is sent to.
#[must_use]
pub fn authorize_url(spec: &OAuth2Spec, config: &Value, client: &OAuthClient, state: &str, code_challenge: Option<&str>) -> String {
    let scope = spec.scopes.join(&spec.scope_separator);
    let mut params: Vec<(String, String)> = vec![
        ("response_type".into(), "code".into()),
        ("client_id".into(), client.client_id.clone()),
        ("redirect_uri".into(), client.redirect_uri.clone()),
        ("scope".into(), scope),
        ("state".into(), state.to_owned()),
    ];
    params.extend(spec.extra_authorize_params.iter().cloned());
    if let Some(challenge) = code_challenge {
        params.push(("code_challenge".into(), challenge.to_owned()));
        params.push(("code_challenge_method".into(), "S256".into()));
    }
    let query: Vec<String> = params.iter().map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v))).collect();
    let base = fill(&spec.authorize_url, config);
    let sep = if base.contains('?') { '&' } else { '?' };
    format!("{base}{sep}{}", query.join("&"))
}

/// Tokens returned by the provider, normalised for storage as connection secrets.
#[derive(Debug, Clone, PartialEq)]
pub struct TokenSet {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    /// The full token response (providers put extra data here, e.g. Zoho `api_domain`).
    pub raw: Value,
}

impl TokenSet {
    /// Secrets JSON. Keeps `previous_refresh` when the provider does not rotate refresh tokens.
    #[must_use]
    pub fn to_secrets(&self, previous_refresh: Option<&str>) -> Value {
        json!({
            "access_token": self.access_token,
            "refresh_token": self.refresh_token.as_deref().or(previous_refresh),
            "expires_at": self.expires_at.map(|t| t.to_rfc3339_opts(SecondsFormat::Secs, true)),
        })
    }
}

async fn token_request(http: &dyn HttpSender, spec: &OAuth2Spec, config: &Value, client: &OAuthClient, params: &[(&str, &str)], now: DateTime<Utc>) -> Result<TokenSet, ConnectorError> {
    let mut form: Vec<(&str, &str)> = params.to_vec();
    let mut req = HttpRequest::post(fill(&spec.token_url, config)).header("Accept", "application/json");
    match spec.client_auth {
        ClientAuth::Body => {
            form.push(("client_id", &client.client_id));
            form.push(("client_secret", &client.client_secret));
        }
        ClientAuth::Basic => {
            let basic = base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", client.client_id, client.client_secret));
            req = req.header("Authorization", format!("Basic {basic}"));
        }
    }
    let resp = http.send(req.form(&form)).await?;
    let body = resp.json().unwrap_or(Value::Null);
    if let Some(err) = body.get("error").and_then(Value::as_str) {
        let message = format!("OAuth error: {err} {}", body.get("error_description").and_then(Value::as_str).unwrap_or(""));
        return Err(if err == "invalid_grant" || err == "invalid_code" || err == "invalid_client" {
            ConnectorError::AuthExpired { message }
        } else {
            ConnectorError::invalid(message)
        });
    }
    if !resp.is_success() {
        return Err(ConnectorError::from_http(&resp));
    }
    let access_token = body
        .get("access_token")
        .and_then(Value::as_str)
        .ok_or_else(|| ConnectorError::permanent("token response has no access_token"))?
        .to_owned();
    let expires_at = body.get("expires_in").and_then(Value::as_i64).and_then(Duration::try_seconds).map(|d| now + d);
    Ok(TokenSet {
        access_token,
        refresh_token: body.get("refresh_token").and_then(Value::as_str).map(str::to_owned),
        expires_at,
        raw: body,
    })
}

/// Exchanges an authorization code for tokens.
pub async fn exchange_code(http: &dyn HttpSender, spec: &OAuth2Spec, config: &Value, client: &OAuthClient, code: &str, verifier: Option<&str>, now: DateTime<Utc>) -> Result<TokenSet, ConnectorError> {
    let mut params = vec![("grant_type", "authorization_code"), ("code", code), ("redirect_uri", client.redirect_uri.as_str())];
    if let Some(v) = verifier {
        params.push(("code_verifier", v));
    }
    token_request(http, spec, config, client, &params, now).await
}

/// Uses a refresh token to get a new access token.
pub async fn refresh(http: &dyn HttpSender, spec: &OAuth2Spec, config: &Value, client: &OAuthClient, refresh_token: &str, now: DateTime<Utc>) -> Result<TokenSet, ConnectorError> {
    token_request(http, spec, config, client, &[("grant_type", "refresh_token"), ("refresh_token", refresh_token)], now).await
}

/// True when the stored token expires within two minutes (or has no known expiry and no token).
#[must_use]
pub fn needs_refresh(secrets: &Value, now: DateTime<Utc>) -> bool {
    let has_token = secrets.get("access_token").and_then(Value::as_str).is_some_and(|t| !t.is_empty());
    let expires = secrets
        .get("expires_at")
        .and_then(Value::as_str)
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|t| t.with_timezone(&Utc));
    !has_token || expires.is_some_and(|t| t <= now + Duration::seconds(120))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_replaces_config_placeholders() {
        let cfg = json!({"accounts_server": "https://accounts.zoho.in"});
        assert_eq!(fill("{{ config.accounts_server }}/oauth/v2/token", &cfg), "https://accounts.zoho.in/oauth/v2/token");
    }

    #[test]
    fn authorize_url_contains_pkce_and_extra_params() {
        let spec = OAuth2Spec {
            authorize_url: "https://accounts.zoho.com/oauth/v2/auth".into(),
            token_url: "x".into(),
            scopes: vec!["A.ALL".into(), "B.READ".into()],
            scope_separator: ",".into(),
            extra_authorize_params: vec![("access_type".into(), "offline".into())],
            pkce: true,
            client_auth: ClientAuth::Body,
        };
        let client = OAuthClient { client_id: "cid".into(), client_secret: "s".into(), redirect_uri: "https://w/oauth/callback".into() };
        let url = authorize_url(&spec, &Value::Null, &client, "st", Some("ch"));
        assert!(url.starts_with("https://accounts.zoho.com/oauth/v2/auth?response_type=code&client_id=cid"));
        assert!(url.contains("scope=A.ALL%2CB.READ") && url.contains("access_type=offline") && url.contains("code_challenge=ch"));
    }

    #[test]
    fn refresh_needed_near_expiry() {
        let now = DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z").unwrap().with_timezone(&Utc);
        assert!(needs_refresh(&json!({"access_token": "a", "expires_at": "2026-01-01T00:01:00Z"}), now));
        assert!(!needs_refresh(&json!({"access_token": "a", "expires_at": "2026-01-01T01:00:00Z"}), now));
        assert!(needs_refresh(&json!({}), now));
    }
}
