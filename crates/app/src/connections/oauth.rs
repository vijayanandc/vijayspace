//! Use cases: start an OAuth authorization and handle the provider's callback.

use std::collections::BTreeMap;

use chrono::Duration;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use weft_connector_sdk::{
    AuthKind, Connector, OAuth2Spec,
    oauth::{self, OAuthClient},
};
use weft_domain::{
    error_codes,
    ids::ConnectionId,
    model::{Connection, ConnectionStatus},
};

use super::{handle::encrypt, manage::split_fields};
use crate::tenancy::Actor;
use crate::{
    AppError, Deps,
    ports::{OAuthState, StoredConnection},
};
use weft_domain::permissions::Permission;

fn state_aad(hash: &[u8]) -> Vec<u8> {
    format!("oauth|{}", hex::encode(hash)).into_bytes()
}

/// Where the browser may be sent after an OAuth sign-in: a page of one of the web client's origins
/// (never an arbitrary site, which would receive the connection ID).
fn return_allowed(origins: &[String], return_to: &str) -> bool {
    if !(return_to.starts_with("http://") || return_to.starts_with("https://")) {
        return false;
    }
    origins.iter().any(|o| {
        let o = o.trim_end_matches('/');
        return_to == o || return_to.starts_with(&format!("{o}/"))
    })
}

/// Returns the provider authorization URL. `config` holds non-secret auth fields (e.g. region).
pub async fn oauth_start(
    deps: &Deps,
    actor: &Actor,
    connector_key: &str,
    name: &str,
    connection_id: Option<ConnectionId>,
    config: &Map<String, Value>,
    return_to: &str,
) -> Result<String, AppError> {
    actor.require(Permission::ConnectionCreate)?;
    let org = actor.org;
    let c = deps
        .connectors
        .get(connector_key)
        .cloned()
        .ok_or_else(|| AppError::not_found(error_codes::CONNECTOR_NOT_FOUND, "Unknown app"))?;
    // Only the app's declared, non-secret settings (e.g. region) are kept: they feed the
    // authorize/token URL templates, so arbitrary keys must not reach them.
    let (declared, secrets) = split_fields(c.as_ref(), config, true)?;
    let config = Value::Object(declared);
    let spec = spec_of(c.as_ref(), &config)?;
    // An app whose connections bring their own OAuth client: its secret comes with this request.
    let own_secret = if c.own_oauth_client() {
        let secret = secrets
            .get("client_secret")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| {
                AppError::validation(
                    error_codes::VALIDATION_FAILED,
                    "Enter the app's client secret",
                )
            })?;
        Some(secret.to_owned())
    } else {
        None
    };
    let client = client_for(
        deps,
        c.as_ref(),
        connector_key,
        &config,
        own_secret.as_deref(),
    )
    .map_err(|m| AppError::validation(error_codes::VALIDATION_FAILED, m))?;
    if !return_allowed(&deps.config.client_origins, return_to) {
        return Err(AppError::validation(
            error_codes::VALIDATION_FAILED,
            "return_to must be a page of this Weft web app",
        ));
    }
    if let Some(id) = connection_id {
        let existing = deps.store.get_connection(org, id).await?.ok_or_else(|| {
            AppError::not_found(error_codes::CONNECTION_NOT_FOUND, "Connection not found")
        })?;
        // Signing a connection in again replaces its account: managing it, like deleting it.
        actor.require_on(Permission::ConnectionDelete, existing.connection.created_by)?;
        if existing.connection.connector != connector_key {
            return Err(AppError::validation(
                error_codes::VALIDATION_FAILED,
                "This connection belongs to another app",
            ));
        }
    }
    let state = oauth::random_token();
    let hash = Sha256::digest(state.as_bytes()).to_vec();
    let (verifier, challenge) = if spec.pkce {
        let (v, c) = oauth::pkce_pair();
        (Some(v), Some(c))
    } else {
        (None, None)
    };
    let verifier_cipher = match &verifier {
        Some(v) => Some(deps.secrets.seal(v.as_bytes(), &state_aad(&hash))?.0),
        None => None,
    };
    let client_cipher = match &own_secret {
        Some(secret) => Some(deps.secrets.seal(secret.as_bytes(), &state_aad(&hash))?.0),
        None => None,
    };
    let now = deps.clock.now();
    deps.store
        .insert_oauth_state(
            &OAuthState {
                state_hash: hash,
                org_id: org,
                connector: connector_key.to_owned(),
                connection_id,
                connection_name: name.trim().chars().take(200).collect(),
                config: config.clone(),
                verifier_cipher,
                return_to: return_to.to_owned(),
                expires_at: now + Duration::minutes(10),
                created_by: actor.user,
                callback_cipher: None,
                finish_hash: None,
                client_cipher,
            },
            now,
        )
        .await?;
    Ok(oauth::authorize_url(
        &spec,
        &config,
        &client,
        &state,
        challenge.as_deref(),
    ))
}

/// The OAuth settings of an app (for a connection with this config), or a validation error.
fn spec_of(c: &dyn Connector, config: &Value) -> Result<OAuth2Spec, AppError> {
    let not_oauth = || {
        AppError::validation(
            error_codes::VALIDATION_FAILED,
            "This app does not use OAuth",
        )
    };
    if c.manifest().auth.kind != AuthKind::Oauth2 {
        return Err(not_oauth());
    }
    c.oauth_spec(config)
        .map_err(|e| AppError::validation(error_codes::VALIDATION_FAILED, e.to_string()))?
        .ok_or_else(not_oauth)
}

/// The OAuth client of a sign-in or a token refresh: the connection's own (`client_id` in its config, the secret
/// given) for apps whose connections bring one, else this server's OAuth app for the connector.
pub(super) fn client_for(
    deps: &Deps,
    c: &dyn Connector,
    connector_key: &str,
    config: &Value,
    own_secret: Option<&str>,
) -> Result<OAuthClient, String> {
    if !c.own_oauth_client() {
        return deps.config.oauth_clients.get(connector_key).cloned().ok_or_else(|| {
            format!(
                "The OAuth app for `{connector_key}` is not configured on this server (set WEFT_OAUTH_{}_CLIENT_ID and _CLIENT_SECRET)",
                connector_key.to_uppercase()
            )
        });
    }
    let client_id = config
        .get("client_id")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or("Enter the app's client ID")?;
    Ok(OAuthClient {
        client_id: client_id.to_owned(),
        client_secret: own_secret.unwrap_or_default().to_owned(),
        redirect_uri: format!(
            "{}/oauth/callback",
            deps.config.public_url.trim_end_matches('/')
        ),
    })
}

fn with_query(url: &str, params: &[(&str, &str)]) -> String {
    let q: Vec<String> = params
        .iter()
        .map(|(k, v)| format!("{k}={}", urlencoding::encode(v)))
        .collect();
    let sep = if url.contains('?') { '&' } else { '?' };
    format!("{url}{sep}{}", q.join("&"))
}

fn invalid(message: &str) -> AppError {
    AppError::validation(error_codes::OAUTH_STATE_INVALID, message)
}

fn callback_aad(hash: &[u8]) -> Vec<u8> {
    format!("oauth-callback|{}", hex::encode(hash)).into_bytes()
}

/// Handles the provider redirect (public: no sign-in), and returns the URL to send the browser to.
/// The provider's answer is only kept here, encrypted, with a one-time finish token sent to this
/// browser: the sign-in completes in [`oauth_finish`], a signed-in request by the person who
/// started it. So a link started in one org and opened by someone else connects nobody's account.
pub async fn oauth_callback(
    deps: &Deps,
    query: &BTreeMap<String, String>,
) -> Result<String, AppError> {
    let state = query.get("state").ok_or_else(|| invalid("Missing state"))?;
    let hash = Sha256::digest(state.as_bytes()).to_vec();
    let now = deps.clock.now();
    let expired = || invalid("This authorization link has expired; start again");
    if let Some(err) = query.get("error") {
        let st = deps
            .store
            .take_oauth_state(&hash, now)
            .await?
            .ok_or_else(expired)?;
        return Ok(with_query(
            &st.return_to,
            &[("status", "error"), ("message", err)],
        ));
    }
    let callback: BTreeMap<&String, &String> =
        query.iter().filter(|(k, _)| *k != "state").collect();
    let plain = serde_json::to_vec(&callback).map_err(|e| AppError::internal(e.to_string()))?;
    let (cipher, _) = deps.secrets.seal(&plain, &callback_aad(&hash))?;
    let finish = oauth::random_token();
    let finish_hash = Sha256::digest(finish.as_bytes()).to_vec();
    let st = deps
        .store
        .park_oauth_callback(&hash, &cipher, &finish_hash, now)
        .await?
        .ok_or_else(expired)?;
    Ok(with_query(
        &st.return_to,
        &[("status", "finish"), ("state", state), ("finish", &finish)],
    ))
}

/// Completes an OAuth sign-in for the person who started it, in the org they started it in, with
/// the finish token their browser got back from the provider.
pub async fn oauth_finish(
    deps: &Deps,
    actor: &Actor,
    state: &str,
    finish: &str,
) -> Result<ConnectionId, AppError> {
    actor.require(Permission::ConnectionCreate)?;
    let hash = Sha256::digest(state.as_bytes()).to_vec();
    let finish_hash = Sha256::digest(finish.as_bytes()).to_vec();
    // Spent only by its owner: someone else's attempt leaves it for the person who started it.
    let st = deps
        .store
        .take_parked_oauth_state(&hash, actor.org, actor.user, &finish_hash, deps.clock.now())
        .await?
        .ok_or_else(|| {
            invalid("This sign-in has expired, or was started by someone else or in another organisation")
        })?;
    let Some(cipher) = st.callback_cipher.as_ref() else {
        return Err(invalid("The app has not answered this sign-in yet"));
    };
    let plain = deps.secrets.open(cipher, "", &callback_aad(&hash))?;
    let query: BTreeMap<String, String> =
        serde_json::from_slice(&plain).map_err(|e| AppError::internal(e.to_string()))?;
    finish_callback(deps, &st, &hash, &query).await
}

/// A value sealed with the sign-in state (the PKCE verifier, an own client secret), opened.
fn opened(deps: &Deps, cipher: Option<&[u8]>, hash: &[u8]) -> Result<Option<String>, AppError> {
    cipher
        .map(|c| {
            deps.secrets
                .open(c, "", &state_aad(hash))
                .map(|plain| String::from_utf8_lossy(&plain).into_owned())
        })
        .transpose()
}

async fn finish_callback(
    deps: &Deps,
    st: &OAuthState,
    hash: &[u8],
    query: &BTreeMap<String, String>,
) -> Result<ConnectionId, AppError> {
    let code = query
        .get("code")
        .ok_or_else(|| AppError::validation(error_codes::OAUTH_STATE_INVALID, "Missing code"))?;
    let c = deps
        .connectors
        .get(&st.connector)
        .cloned()
        .ok_or_else(|| AppError::not_found(error_codes::CONNECTOR_NOT_FOUND, "Unknown app"))?;
    let mut config = st.config.as_object().cloned().unwrap_or_default();
    if let Value::Object(extra) = c.oauth_callback_config(query) {
        config.extend(extra.into_iter().filter(|(_, v)| !v.is_null()));
    }
    let verifier = opened(deps, st.verifier_cipher.as_deref(), hash)?;
    let own_secret = opened(deps, st.client_cipher.as_deref(), hash)?;
    let config_value = Value::Object(config.clone());
    let spec = spec_of(c.as_ref(), &config_value)?;
    let client = client_for(
        deps,
        c.as_ref(),
        &st.connector,
        &config_value,
        own_secret.as_deref(),
    )
    .map_err(|m| AppError::validation(error_codes::VALIDATION_FAILED, m))?;
    let now = deps.clock.now();
    let tokens = oauth::exchange_code(
        deps.http.as_ref(),
        &spec,
        &Value::Object(config.clone()),
        &client,
        code,
        verifier.as_deref(),
        now,
    )
    .await
    .map_err(|e| AppError::validation(error_codes::VALIDATION_FAILED, e.to_string()))?;
    if let Value::Object(extra) = c.oauth_token_config(&tokens.raw) {
        config.extend(extra.into_iter().filter(|(_, v)| !v.is_null()));
    }
    let mut secrets = tokens
        .to_secrets(None)
        .as_object()
        .cloned()
        .unwrap_or_default();
    // The connection's own client secret is kept with its tokens: refreshing needs it.
    if let Some(secret) = own_secret {
        secrets.insert("client_secret".into(), Value::String(secret));
    }
    let existing = match st.connection_id {
        Some(id) => deps.store.get_connection(st.org_id, id).await?,
        None => None,
    };
    if existing
        .as_ref()
        .is_some_and(|s| s.connection.connector != st.connector)
    {
        return Err(invalid("This connection belongs to another app"));
    }
    let connection = existing.as_ref().map_or_else(
        || Connection {
            id: deps.new_connection_id(),
            org_id: st.org_id,
            connector: st.connector.clone(),
            name: st.connection_name.clone(),
            config: Value::Object(config.clone()),
            status: ConnectionStatus::Active,
            created_at: now,
            updated_at: now,
            created_by: st.created_by,
        },
        |s| {
            let mut c = s.connection.clone();
            c.config = Value::Object(config.clone());
            c.status = ConnectionStatus::Active;
            c.updated_at = now;
            c
        },
    );
    let (ciphertext, key_id) = encrypt(deps, &connection, &secrets)?;
    let stored = StoredConnection {
        connection: connection.clone(),
        ciphertext,
        key_id,
    };
    if existing.is_some() {
        deps.store.update_connection(&stored).await?;
    } else {
        deps.store.insert_connection(&stored).await?;
    }
    let _ = super::manage::check_connection(deps, st.org_id, connection.id).await;
    Ok(connection.id)
}
