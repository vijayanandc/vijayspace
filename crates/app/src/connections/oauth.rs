//! Use cases: start an OAuth authorization and handle the provider's callback.

use std::collections::BTreeMap;

use chrono::Duration;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use weft_connector_sdk::{AuthKind, oauth};
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
    let spec = match (&c.manifest().auth.kind, &c.manifest().auth.oauth2) {
        (AuthKind::Oauth2, Some(spec)) => spec.clone(),
        _ => {
            return Err(AppError::validation(
                error_codes::VALIDATION_FAILED,
                "This app does not use OAuth",
            ));
        }
    };
    let client = deps.config.oauth_clients.get(connector_key).cloned().ok_or_else(|| {
        AppError::validation(
            error_codes::VALIDATION_FAILED,
            format!("The OAuth app for `{connector_key}` is not configured on this server (set WEFT_OAUTH_{}_CLIENT_ID and _CLIENT_SECRET)", connector_key.to_uppercase()),
        )
    })?;
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
    // Only the app's declared, non-secret settings (e.g. region) are kept: they feed the
    // authorize/token URL templates, so arbitrary keys must not reach them.
    let (declared, _secrets) = split_fields(c.as_ref(), config, true)?;
    let now = deps.clock.now();
    let config = Value::Object(declared);
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

fn with_query(url: &str, params: &[(&str, &str)]) -> String {
    let q: Vec<String> = params
        .iter()
        .map(|(k, v)| format!("{k}={}", urlencoding::encode(v)))
        .collect();
    let sep = if url.contains('?') { '&' } else { '?' };
    format!("{url}{sep}{}", q.join("&"))
}

/// Handles the provider redirect; returns the URL to send the browser to.
pub async fn oauth_callback(
    deps: &Deps,
    query: &BTreeMap<String, String>,
) -> Result<String, AppError> {
    let state = query
        .get("state")
        .ok_or_else(|| AppError::validation(error_codes::OAUTH_STATE_INVALID, "Missing state"))?;
    let hash = Sha256::digest(state.as_bytes()).to_vec();
    let now = deps.clock.now();
    let st = deps
        .store
        .take_oauth_state(&hash, now)
        .await?
        .ok_or_else(|| {
            AppError::validation(
                error_codes::OAUTH_STATE_INVALID,
                "This authorization link has expired; start again",
            )
        })?;
    if let Some(err) = query.get("error") {
        return Ok(with_query(
            &st.return_to,
            &[("status", "error"), ("message", err)],
        ));
    }
    match finish_callback(deps, &st, &hash, query).await {
        Ok(id) => Ok(with_query(
            &st.return_to,
            &[("status", "connected"), ("connection_id", &id.to_string())],
        )),
        Err(e) => Ok(with_query(
            &st.return_to,
            &[("status", "error"), ("message", &e.message)],
        )),
    }
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
    let spec = c.manifest().auth.oauth2.clone().ok_or_else(|| {
        AppError::validation(
            error_codes::VALIDATION_FAILED,
            "This app does not use OAuth",
        )
    })?;
    let client = deps
        .config
        .oauth_clients
        .get(&st.connector)
        .cloned()
        .ok_or_else(|| {
            AppError::validation(error_codes::VALIDATION_FAILED, "OAuth app not configured")
        })?;
    let mut config = st.config.as_object().cloned().unwrap_or_default();
    if let Value::Object(extra) = c.oauth_callback_config(query) {
        config.extend(extra.into_iter().filter(|(_, v)| !v.is_null()));
    }
    let verifier = match &st.verifier_cipher {
        Some(cipher) => Some(
            String::from_utf8_lossy(&deps.secrets.open(cipher, "", &state_aad(hash))?).into_owned(),
        ),
        None => None,
    };
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
    let secrets = tokens
        .to_secrets(None)
        .as_object()
        .cloned()
        .unwrap_or_default();
    let existing = match st.connection_id {
        Some(id) => deps.store.get_connection(st.org_id, id).await?,
        None => None,
    };
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
