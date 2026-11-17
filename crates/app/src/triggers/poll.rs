//! Polling app triggers and reconciliation polls of instant triggers.

use chrono::Duration;
use serde_json::json;
use weft_connector_sdk::ConnectorError;
use weft_domain::{
    flow::TriggerNode,
    ids::RegistrationId,
    model::{ExecutionMode, RegistrationKind},
};

use super::{
    ingress::start_items,
    registration::{DEFAULT_POLL_SECS, DEFAULT_RECONCILE_SECS},
};
use crate::{
    AppError, Deps,
    ports::NewJob,
    runner::action::{action_ctx, load_connection},
};

/// Enqueues a `poll_trigger` job for each due poll/instant registration.
pub async fn enqueue_due_polls(deps: &Deps) -> Result<usize, AppError> {
    let now = deps.clock.now();
    let due = deps
        .store
        .take_due(&["poll", "instant"], now, now + Duration::minutes(5), 100)
        .await?;
    let mut n = 0;
    for reg in due {
        let slot = reg.next_fire_at.unwrap_or(now).timestamp();
        let job = NewJob {
            id: deps.new_job_id(),
            queue: "triggers".into(),
            kind: "poll_trigger".into(),
            org_id: Some(reg.org_id),
            payload: json!({"registration_id": reg.id}),
            run_at: now,
            dedupe_key: Some(format!("poll:{}:{slot}", reg.id)),
        };
        if deps.store.enqueue(&job, now).await? {
            n += 1;
        }
        let secs = if reg.kind == RegistrationKind::Poll {
            DEFAULT_POLL_SECS
        } else {
            DEFAULT_RECONCILE_SECS
        };
        let interval = poll_interval(deps, &reg).await.unwrap_or(secs);
        deps.store
            .set_next_fire(reg.id, Some(now + Duration::seconds(interval)), now)
            .await?;
    }
    if n > 0 {
        deps.wake.notify_one();
    }
    Ok(n)
}

async fn poll_interval(deps: &Deps, reg: &weft_domain::model::TriggerRegistration) -> Option<i64> {
    let flow = deps.store.get_flow_system(reg.flow_id).await.ok()??;
    let version = deps
        .store
        .get_version(flow.published_version_id?)
        .await
        .ok()??;
    match &version.definition.trigger {
        TriggerNode::App(app) => app.poll_interval_secs.map(|s| i64::from(s).max(60)),
        TriggerNode::Webhook(_) | TriggerNode::Schedule(_) | TriggerNode::Manual => None,
    }
}

/// Runs one poll: asks the connector for changes since the stored cursor and starts executions.
pub async fn run_poll(deps: &Deps, id: RegistrationId) -> Result<usize, AppError> {
    let Some(reg) = deps.store.get_registration(id).await? else {
        return Ok(0);
    };
    let Some(flow) = deps.store.get_flow_system(reg.flow_id).await? else {
        return Ok(0);
    };
    let Some(version_id) = flow.published_version_id else {
        return Ok(0);
    };
    let Some(version) = deps.store.get_version(version_id).await? else {
        return Ok(0);
    };
    if !reg.is_active {
        return Ok(0);
    }
    let TriggerNode::App(app) = &version.definition.trigger else {
        return Ok(0);
    };
    let Some(connector) = deps.connectors.get(&app.connector).cloned() else {
        return Ok(0);
    };
    let connection = load_connection(deps, reg.org_id, app.connection_id.as_deref())
        .await
        .map_err(|e| {
            AppError::validation(
                weft_domain::error_codes::CONNECTION_NOT_FOUND,
                e.to_string(),
            )
        })?;
    let ctx = action_ctx(deps, reg.org_id, connection, String::new());
    let cursor = reg.state.get("cursor").cloned().filter(|c| !c.is_null());
    let result = match connector
        .poll(&ctx, &app.trigger, &app.input, cursor.as_ref())
        .await
    {
        Ok(r) => r,
        Err(ConnectorError::Unsupported { .. }) => return Ok(0),
        Err(e) => {
            return Err(AppError::new(
                crate::ErrorKind::BadGateway,
                e.code(),
                e.to_string(),
            ));
        }
    };
    let now = deps.clock.now();
    if reg.listening_until.is_some_and(|u| u > now)
        && let Some(first) = result.items.first()
    {
        deps.store.set_sample(reg.id, &first.data, now).await?;
    }
    let created = start_items(
        deps,
        &reg,
        &flow,
        &version,
        &result.items,
        app.debounce_secs,
        ExecutionMode::Live,
        now,
    )
    .await?;
    if let Some(cursor) = result.cursor {
        let mut state = reg.state.clone();
        if !state.is_object() {
            state = json!({});
        }
        state["cursor"] = cursor;
        deps.store.set_state(reg.id, &state, now).await?;
    }
    Ok(created.len())
}
