//! JWT access tokens for the configured API resource, verified locally (doc 08 §1): the signature
//! against the tenant's JWKS (refetched on an unknown `kid`, at most once a minute), `iss`, `aud`,
//! `exp`/`nbf` with 60 s leeway on the injected clock, and an algorithm allowlist.

use std::sync::RwLock;

use chrono::{DateTime, Duration, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header, jwk::JwkSet};
use serde::Deserialize;
use weft_app::ports::{Res, VerifiedIdentity};

use super::{LogtoVerifier, unavailable};

const LEEWAY_SECS: i64 = 60;
const ALLOWED: [Algorithm; 3] = [Algorithm::ES256, Algorithm::ES384, Algorithm::RS256];

#[derive(Debug, Default)]
pub(super) struct Jwks {
    keys: RwLock<Option<JwkSet>>,
    fetched_at: RwLock<Option<DateTime<Utc>>>,
}

#[derive(Debug, Deserialize)]
struct Claims {
    sub: String,
    exp: i64,
    #[serde(default)]
    nbf: Option<i64>,
    #[serde(default)]
    client_id: Option<String>,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    email_verified: Option<bool>,
    #[serde(default)]
    name: Option<String>,
}

impl LogtoVerifier {
    async fn key(&self, kid: &str) -> Res<Option<DecodingKey>> {
        let cached = self
            .jwks
            .keys
            .read()
            .ok()
            .and_then(|k| k.as_ref().and_then(|set| set.find(kid).cloned()));
        if let Some(jwk) = cached {
            return Ok(DecodingKey::from_jwk(&jwk).ok());
        }
        let now = self.clock.now();
        let recent = self
            .jwks
            .fetched_at
            .read()
            .ok()
            .and_then(|t| *t)
            .is_some_and(|t| now - t < Duration::seconds(60));
        if recent {
            return Ok(None);
        }
        let set: JwkSet = self
            .http
            .get(format!("{}/jwks", self.issuer))
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|e| unavailable(&format!("fetching the JWKS: {e}")))?
            .json()
            .await
            .map_err(|e| unavailable(&format!("reading the JWKS: {e}")))?;
        let key = set
            .find(kid)
            .and_then(|jwk| DecodingKey::from_jwk(jwk).ok());
        if let Ok(mut k) = self.jwks.keys.write() {
            *k = Some(set);
        }
        if let Ok(mut t) = self.jwks.fetched_at.write() {
            *t = Some(now);
        }
        Ok(key)
    }

    /// A verified JWT's identity and expiry, or `None` when it is not valid for this API.
    pub(super) async fn verify_jwt(
        &self,
        token: &str,
    ) -> Res<Option<(VerifiedIdentity, DateTime<Utc>)>> {
        let Some(resource) = &self.settings.resource else {
            return Ok(None);
        };
        let Ok(header) = decode_header(token) else {
            return Ok(None);
        };
        let Some(kid) = header
            .kid
            .as_deref()
            .filter(|_| ALLOWED.contains(&header.alg))
        else {
            return Ok(None);
        };
        let Some(key) = self.key(kid).await? else {
            return Ok(None);
        };
        let mut validation = Validation::new(header.alg);
        validation.set_issuer(&[&self.issuer]);
        validation.set_audience(&[resource]);
        validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
        // Time is checked below on the injected clock.
        validation.validate_exp = false;
        validation.validate_nbf = false;
        let Ok(data) = decode::<Claims>(token, &key, &validation) else {
            return Ok(None);
        };
        let c = data.claims;
        let now = self.clock.now().timestamp();
        let fresh = c.exp + LEEWAY_SECS > now && c.nbf.is_none_or(|nbf| nbf - LEEWAY_SECS <= now);
        if !fresh || c.sub.trim().is_empty() || !self.client_allowed(c.client_id.as_deref()) {
            return Ok(None);
        }
        let Some(expires) = DateTime::from_timestamp(c.exp, 0) else {
            return Ok(None);
        };
        Ok(Some((
            VerifiedIdentity {
                subject: c.sub,
                email: c.email,
                email_verified: c.email_verified.unwrap_or(false),
                name: c.name,
            },
            expires,
        )))
    }
}
