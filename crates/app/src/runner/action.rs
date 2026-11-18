//! Calling one connector action for a step (connection loading, timeout, error mapping).

use std::{sync::Arc, time::Duration};

use serde_json::Value;
use sha2::{Digest, Sha256};
use weft_connector_sdk::{ActionCtx, ConnectionHandle, ConnectorError};
use weft_domain::{
    error_codes,
    ids::{ConnectionId, OrgId},
};
use weft_engine::StepError;

use crate::{Deps, connections::handle::DbConnectionHandle, sync};

/// Maximum serialized step output kept (doc 01 §4).
pub const MAX_OUTPUT_BYTES: usize = 1024 * 1024;
const ACTION_TIMEOUT: Duration = Duration::from_secs(30);

/// Converts a connector error into the engine's step error.
#[must_use]
pub fn step_error(err: &ConnectorError) -> StepError {
    StepError {
        code: err.code().to_owned(),
        message: err.to_string(),
        retryable: err.retryable(),
        retry_after_secs: err.retry_after_secs(),
        details: err.details(),
    }
}

/// Loads a connection handle for `connection_id` (scoped to `org`).
pub async fn load_connection(
    deps: &Deps,
    org: OrgId,
    connection_id: Option<&str>,
) -> Result<Option<Arc<dyn ConnectionHandle>>, ConnectorError> {
    let Some(raw) = connection_id.filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let id: ConnectionId = raw
        .parse()
        .map_err(|_| ConnectorError::invalid_field("connection_id", "Invalid connection"))?;
    let stored = deps
        .store
        .get_connection(org, id)
        .await
        .map_err(|e| ConnectorError::transient(format!("loading connection: {e}")))?
        .ok_or_else(|| {
            ConnectorError::invalid_field("connection_id", "The connection no longer exists")
        })?;
    Ok(Some(Arc::new(DbConnectionHandle::new(
        deps.clone(),
        stored,
    ))))
}

/// Builds an action context.
#[must_use]
pub fn action_ctx(
    deps: &Deps,
    org: OrgId,
    connection: Option<Arc<dyn ConnectionHandle>>,
    idempotency_key: String,
) -> ActionCtx {
    ActionCtx {
        org_id: org.to_string(),
        http: deps.http.clone(),
        connection,
        idempotency_key,
        mailer: deps.mailer.clone(),
        scripts: deps.scripts.clone(),
        timeout: ACTION_TIMEOUT,
        now: deps.clock.now(),
    }
}

/// Idempotency key for one step of one execution (stable across retries).
#[must_use]
pub fn idempotency_key(execution: &str, step: &str) -> String {
    hex::encode(Sha256::digest(format!("{execution}|{step}").as_bytes()))
}

/// Runs `connector.action` with rendered input. Never panics; all failures are `ConnectorError`s.
pub async fn call_action(
    deps: &Deps,
    org: OrgId,
    connector_key: &str,
    action: &str,
    connection_id: Option<&str>,
    input: Value,
    idem: String,
) -> Result<Value, ConnectorError> {
    if connector_key == sync::CONNECTOR_KEY {
        return sync::execute(deps, org, action, &input).await;
    }
    let connector = deps
        .connectors
        .get(connector_key)
        .cloned()
        .ok_or_else(|| ConnectorError::invalid(format!("Unknown app `{connector_key}`")))?;
    let connection = if connector.manifest().requires_connection() {
        let handle = load_connection(deps, org, connection_id).await?;
        if handle.is_none() {
            return Err(ConnectorError::invalid_field(
                "connection_id",
                "Choose a connection for this step",
            ));
        }
        handle
    } else {
        None
    };
    let ctx = action_ctx(deps, org, connection, idem);
    let output = match tokio::time::timeout(
        ACTION_TIMEOUT + Duration::from_secs(5),
        connector.execute(&ctx, action, input),
    )
    .await
    {
        Ok(result) => result?,
        Err(_) => return Err(ConnectorError::transient("the action timed out")),
    };
    if output.to_string().len() > MAX_OUTPUT_BYTES {
        return Err(ConnectorError::InvalidInput {
            field: None,
            message: format!(
                "{}: the output is larger than 1 MB",
                error_codes::STEP_OUTPUT_TOO_LARGE
            ),
        });
    }
    Ok(output)
}
