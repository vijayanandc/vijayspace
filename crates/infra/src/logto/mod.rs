//! Logto sign-in (doc 13 §6, doc 08 §1): checks the access tokens the web client gets from the
//! Logto tenant.
//! - **JWTs** (when an API resource is configured): verified locally, see [`jwt`].
//! - **Opaque tokens** (no resource, as the Appblocks web apps use today): OAuth 2.0 token
//!   introspection with this server's confidential app, then userinfo for the email and name.
//!
//! Only tokens issued to the configured client apps are accepted. Results are cached by token
//! hash for at most `cache_ttl`, and never past the token's expiry.

mod cache;
mod jwt;
#[cfg(test)]
mod tests;

use std::{sync::Arc, time::Duration as StdDuration};

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use weft_app::{
    AppError, ErrorKind,
    ports::{Clock, IdentityVerifier, Res, VerifiedIdentity},
};
use weft_domain::error_codes;

use cache::{TokenCache, token_key};

/// Longest token accepted (anything longer is not a Logto token).
const MAX_TOKEN_BYTES: usize = 8 * 1024;

/// Where and how to check tokens.
#[derive(Clone)]
pub struct LogtoSettings {
    /// The tenant, e.g. `https://tenant.logto.app`.
    pub endpoint: String,
    /// This server's confidential (traditional web) app, used for introspection.
    pub app_id: String,
    pub app_secret: String,
    /// The API resource JWTs must be issued for; `None` accepts opaque tokens only.
    pub resource: Option<String>,
    /// Apps whose tokens are accepted (the web client); empty accepts any app of the tenant.
    pub client_ids: Vec<String>,
    /// Scopes an opaque token must carry, e.g. `openid profile email`.
    pub required_scopes: Vec<String>,
    pub cache_ttl: StdDuration,
    pub cache_size: usize,
}

impl std::fmt::Debug for LogtoSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LogtoSettings")
            .field("endpoint", &self.endpoint)
            .field("app_id", &self.app_id)
            .field("resource", &self.resource)
            .field("client_ids", &self.client_ids)
            .finish_non_exhaustive()
    }
}

/// Checks Logto access tokens.
pub struct LogtoVerifier {
    http: reqwest::Client,
    settings: LogtoSettings,
    issuer: String,
    clock: Arc<dyn Clock>,
    jwks: jwt::Jwks,
    cache: TokenCache,
}

impl std::fmt::Debug for LogtoVerifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LogtoVerifier")
            .field("issuer", &self.issuer)
            .finish_non_exhaustive()
    }
}

/// The provider could not be asked: the request fails, but not as "signed out".
fn unavailable(detail: &str) -> AppError {
    tracing::warn!(detail, "Logto could not be reached");
    AppError::new(
        ErrorKind::BadGateway,
        error_codes::AUTH_UNAVAILABLE,
        "Sign-in could not be checked right now. Try again in a moment.",
    )
}

#[derive(Debug, Deserialize)]
struct Introspection {
    active: bool,
    #[serde(default)]
    sub: Option<String>,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    iss: Option<String>,
    #[serde(default)]
    exp: Option<i64>,
    #[serde(default)]
    client_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UserInfo {
    sub: String,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    email_verified: Option<bool>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    username: Option<String>,
}

impl LogtoVerifier {
    pub fn new(settings: LogtoSettings, clock: Arc<dyn Clock>) -> Result<Self, String> {
        let endpoint = settings.endpoint.trim().trim_end_matches('/').to_owned();
        if endpoint.is_empty() || settings.app_id.trim().is_empty() {
            return Err("the Logto endpoint and app ID are required".into());
        }
        let http = reqwest::Client::builder()
            .timeout(StdDuration::from_secs(10))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            http,
            issuer: format!("{endpoint}/oidc"),
            cache: TokenCache::new(settings.cache_size),
            settings,
            clock,
            jwks: jwt::Jwks::default(),
        })
    }

    fn client_allowed(&self, client_id: Option<&str>) -> bool {
        self.settings.client_ids.is_empty()
            || client_id.is_some_and(|c| self.settings.client_ids.iter().any(|a| a == c))
    }

    fn scopes_granted(&self, scope: Option<&str>) -> bool {
        let granted: Vec<&str> = scope.unwrap_or_default().split_whitespace().collect();
        self.settings
            .required_scopes
            .iter()
            .all(|s| granted.contains(&s.as_str()))
    }

    async fn introspect(&self, token: &str) -> Res<Introspection> {
        let response = self
            .http
            .post(format!("{}/token/introspection", self.issuer))
            .basic_auth(&self.settings.app_id, Some(&self.settings.app_secret))
            .form(&[("token", token), ("token_type_hint", "access_token")])
            .send()
            .await
            .map_err(|e| unavailable(&format!("introspection: {e}")))?;
        if !response.status().is_success() {
            // 401 here means this server's own app credentials are wrong: a configuration error.
            return Err(unavailable(&format!(
                "introspection returned {}",
                response.status()
            )));
        }
        response
            .json()
            .await
            .map_err(|e| unavailable(&format!("introspection response: {e}")))
    }

    async fn user_info(&self, token: &str) -> Res<Option<UserInfo>> {
        let response = self
            .http
            .get(format!("{}/me", self.issuer))
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| unavailable(&format!("userinfo: {e}")))?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(unavailable(&format!(
                "userinfo returned {}",
                response.status()
            )));
        }
        response
            .json()
            .await
            .map(Some)
            .map_err(|e| unavailable(&format!("userinfo response: {e}")))
    }

    /// An opaque token's identity and expiry, or `None` when Logto does not vouch for it.
    async fn verify_opaque(&self, token: &str) -> Res<Option<(VerifiedIdentity, DateTime<Utc>)>> {
        if self.settings.app_secret.trim().is_empty() {
            return Ok(None);
        }
        let i = self.introspect(token).await?;
        let issuer_ok = i.iss.as_deref() == Some(self.issuer.as_str());
        let now = self.clock.now();
        let expires = i.exp.and_then(|exp| DateTime::from_timestamp(exp, 0));
        let Some(subject) = i.sub.filter(|s| !s.trim().is_empty()) else {
            return Ok(None);
        };
        let valid = i.active
            && issuer_ok
            && expires.is_some_and(|e| e > now)
            && self.client_allowed(i.client_id.as_deref())
            && self.scopes_granted(i.scope.as_deref());
        let Some(expires) = expires.filter(|_| valid) else {
            return Ok(None);
        };
        let Some(info) = self.user_info(token).await? else {
            return Ok(None);
        };
        if info.sub != subject {
            return Ok(None);
        }
        Ok(Some((
            VerifiedIdentity {
                subject,
                email: info.email,
                email_verified: info.email_verified.unwrap_or(false),
                name: info.name.or(info.username),
            },
            expires,
        )))
    }
}

#[async_trait]
impl IdentityVerifier for LogtoVerifier {
    async fn verify(&self, token: &str) -> Res<Option<VerifiedIdentity>> {
        if token.is_empty() || token.len() > MAX_TOKEN_BYTES {
            return Ok(None);
        }
        let key = token_key(token);
        let now = self.clock.now();
        if let Some(identity) = self.cache.get(&key, now) {
            return Ok(Some(identity));
        }
        let verified = if token.matches('.').count() == 2 {
            self.verify_jwt(token).await?
        } else {
            self.verify_opaque(token).await?
        };
        let Some((identity, expires)) = verified else {
            return Ok(None);
        };
        let ttl = Duration::from_std(self.settings.cache_ttl).unwrap_or_else(|_| Duration::zero());
        self.cache
            .put(key, identity.clone(), expires.min(now + ttl), now);
        Ok(Some(identity))
    }
}
