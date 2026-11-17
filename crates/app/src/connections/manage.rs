//! Use cases: create / list / get / update / delete / test connections; dynamic options and fields.

use std::sync::Arc;

use serde::Serialize;
use serde_json::{Map, Value, json};
use weft_connector_sdk::{AuthKind, Connector, ConnectorError, FieldSpec, OptionItem};
use weft_domain::{
    error_codes,
    ids::{ConnectionId, OrgId},
    model::{Connection, ConnectionStatus},
};

use super::handle::{DbConnectionHandle, decrypt, encrypt};
use crate::{
    AppError, Deps,
    ports::StoredConnection,
    runner::{action::action_ctx, step_error},
};

fn connector(deps: &Deps, key: &str) -> Result<Arc<dyn Connector>, AppError> {
    deps.connectors.get(key).cloned().ok_or_else(|| {
        AppError::not_found(
            error_codes::CONNECTOR_NOT_FOUND,
            format!("Unknown app `{key}`"),
        )
    })
}

/// Non-secret config and secret fields of a connection form.
type SplitFields = (Map<String, Value>, Map<String, Value>);

/// Splits submitted auth fields into (config, secrets) by the manifest; checks required ones.
fn split_fields(
    connector: &dyn Connector,
    fields: &Map<String, Value>,
    partial: bool,
) -> Result<SplitFields, AppError> {
    let mut config = Map::new();
    let mut secrets = Map::new();
    for spec in &connector.manifest().auth.fields {
        let value = fields
            .get(&spec.key)
            .filter(|v| !v.is_null() && v.as_str() != Some(""));
        match value {
            Some(v) if spec.secret => {
                secrets.insert(spec.key.clone(), v.clone());
            }
            Some(v) => {
                config.insert(spec.key.clone(), v.clone());
            }
            None if spec.required && !partial => {
                return Err(AppError::validation(
                    error_codes::VALIDATION_FAILED,
                    format!("`{}` is required", spec.label),
                ));
            }
            None => {
                if let Some(default) = spec.default.clone().filter(|_| !spec.secret && !partial) {
                    config.insert(spec.key.clone(), default);
                }
            }
        }
    }
    Ok((config, secrets))
}

/// Creates a connection from form fields (auth kind `fields`) and tests it.
pub async fn create_connection(
    deps: &Deps,
    org: OrgId,
    connector_key: &str,
    name: &str,
    fields: &Map<String, Value>,
) -> Result<Connection, AppError> {
    let c = connector(deps, connector_key)?;
    if c.manifest().auth.kind != AuthKind::Fields {
        return Err(AppError::validation(
            error_codes::VALIDATION_FAILED,
            "This app connects with OAuth (or needs no connection)",
        ));
    }
    let (config, secrets) = split_fields(c.as_ref(), fields, false)?;
    let now = deps.clock.now();
    let connection = Connection {
        id: deps.new_connection_id(),
        org_id: org,
        connector: connector_key.to_owned(),
        name: name.trim().chars().take(200).collect(),
        config: Value::Object(config),
        status: ConnectionStatus::Active,
        created_at: now,
        updated_at: now,
    };
    let (ciphertext, key_id) = encrypt(deps, &connection, &secrets)?;
    deps.store
        .insert_connection(&StoredConnection {
            connection: connection.clone(),
            ciphertext,
            key_id,
        })
        .await?;
    let _ = test_connection(deps, org, connection.id).await?;
    get_connection(deps, org, connection.id).await
}

pub async fn list_connections(deps: &Deps, org: OrgId) -> Result<Vec<Connection>, AppError> {
    deps.store.list_connections(org).await
}

pub async fn get_connection(
    deps: &Deps,
    org: OrgId,
    id: ConnectionId,
) -> Result<Connection, AppError> {
    Ok(stored(deps, org, id).await?.connection)
}

async fn stored(deps: &Deps, org: OrgId, id: ConnectionId) -> Result<StoredConnection, AppError> {
    deps.store.get_connection(org, id).await?.ok_or_else(|| {
        AppError::not_found(error_codes::CONNECTION_NOT_FOUND, "Connection not found")
    })
}

/// Renames a connection and/or replaces some of its fields (empty secret fields keep their value).
pub async fn update_connection(
    deps: &Deps,
    org: OrgId,
    id: ConnectionId,
    name: Option<&str>,
    fields: Option<&Map<String, Value>>,
) -> Result<Connection, AppError> {
    let mut s = stored(deps, org, id).await?;
    if let Some(n) = name.map(str::trim).filter(|n| !n.is_empty()) {
        s.connection.name = n.chars().take(200).collect();
    }
    if let Some(fields) = fields {
        let c = connector(deps, &s.connection.connector)?;
        let (config, new_secrets) = split_fields(c.as_ref(), fields, true)?;
        if let Some(cfg) = s.connection.config.as_object_mut() {
            cfg.extend(config);
        }
        if !new_secrets.is_empty() {
            let mut secrets = decrypt(deps, &s)
                .map_err(|e| AppError::internal(e.to_string()))?
                .as_object()
                .cloned()
                .unwrap_or_default();
            secrets.extend(new_secrets);
            let (ciphertext, key_id) = encrypt(deps, &s.connection, &secrets)?;
            s.ciphertext = ciphertext;
            s.key_id = key_id;
        }
    }
    s.connection.updated_at = deps.clock.now();
    deps.store.update_connection(&s).await?;
    Ok(s.connection)
}

pub async fn delete_connection(deps: &Deps, org: OrgId, id: ConnectionId) -> Result<(), AppError> {
    stored(deps, org, id).await?;
    deps.store
        .soft_delete_connection(org, id, deps.clock.now())
        .await
}

/// Result of testing a connection.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ConnectionTest {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub info: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
}

/// Calls the connector's credential check and records the connection status.
pub async fn test_connection(
    deps: &Deps,
    org: OrgId,
    id: ConnectionId,
) -> Result<ConnectionTest, AppError> {
    let s = stored(deps, org, id).await?;
    let c = connector(deps, &s.connection.connector)?;
    let handle = Arc::new(DbConnectionHandle::new(deps.clone(), s.clone()));
    let ctx = action_ctx(deps, org, Some(handle), String::new());
    let now = deps.clock.now();
    match c.test_connection(&ctx).await {
        Ok(info) => {
            let mut s = stored(deps, org, id).await?;
            if let Some(cfg) = s.connection.config.as_object_mut()
                && !info.is_null()
            {
                cfg.insert("account".into(), info.clone());
            }
            s.connection.status = ConnectionStatus::Active;
            s.connection.updated_at = now;
            deps.store.update_connection(&s).await?;
            Ok(ConnectionTest {
                ok: true,
                info: Some(info),
                error: None,
            })
        }
        Err(err) => {
            let status = if matches!(err, ConnectorError::AuthExpired { .. }) {
                ConnectionStatus::NeedsReauth
            } else {
                ConnectionStatus::Error
            };
            deps.store.set_connection_status(id, status, now).await?;
            let e = step_error(&err);
            Ok(ConnectionTest {
                ok: false,
                info: None,
                error: Some(json!({"code": e.code, "message": e.message})),
            })
        }
    }
}

async fn ctx_for(
    deps: &Deps,
    org: OrgId,
    connection_id: Option<&str>,
) -> Result<weft_connector_sdk::ActionCtx, AppError> {
    let handle = crate::runner::action::load_connection(deps, org, connection_id)
        .await
        .map_err(|e| AppError::validation(error_codes::CONNECTION_NOT_FOUND, e.to_string()))?;
    Ok(action_ctx(deps, org, handle, String::new()))
}

fn bad_gateway(err: &ConnectorError) -> AppError {
    AppError::new(crate::ErrorKind::BadGateway, err.code(), err.to_string())
}

/// Dynamic dropdown options.
pub async fn load_options(
    deps: &Deps,
    org: OrgId,
    connector_key: &str,
    source: &str,
    connection_id: Option<&str>,
    input: &Value,
) -> Result<Vec<OptionItem>, AppError> {
    if connector_key == crate::sync::CONNECTOR_KEY {
        return crate::sync::options(deps, org, source, input)
            .await
            .map_err(|e| bad_gateway(&e));
    }
    let c = connector(deps, connector_key)?;
    let ctx = ctx_for(deps, org, connection_id).await?;
    c.options(&ctx, source, input)
        .await
        .map_err(|e| bad_gateway(&e))
}

/// Dynamic sub-fields (e.g. a module's fields).
pub async fn load_fields(
    deps: &Deps,
    org: OrgId,
    connector_key: &str,
    source: &str,
    connection_id: Option<&str>,
    input: &Value,
) -> Result<Vec<FieldSpec>, AppError> {
    if connector_key == crate::sync::CONNECTOR_KEY {
        return crate::sync::fields(deps, org, source, input)
            .await
            .map_err(|e| bad_gateway(&e));
    }
    let c = connector(deps, connector_key)?;
    let ctx = ctx_for(deps, org, connection_id).await?;
    c.fields(&ctx, source, input)
        .await
        .map_err(|e| bad_gateway(&e))
}
