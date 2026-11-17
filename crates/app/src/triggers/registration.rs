//! Keeps a flow's trigger registration (and provider subscription) in line with its state.

use chrono::Duration;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use weft_connector_sdk::{ConnectorError, ConnectorRegistry, SubscribeRequest, TriggerKind};
use weft_domain::{
    flow::{FlowDefinition, TriggerNode},
    model::{Flow, RegistrationKind, TriggerRegistration},
};

use crate::{
    AppError, Deps,
    flows::manage_urls::app_event_url,
    ports::Subscription,
    runner::action::{action_ctx, load_connection},
};

/// Default polling interval for polling triggers, and reconciliation interval for instant ones.
pub const DEFAULT_POLL_SECS: i64 = 300;
pub const DEFAULT_RECONCILE_SECS: i64 = 3_600;

/// The runtime kind of a definition's trigger.
#[must_use]
pub fn registration_kind(def: &FlowDefinition, registry: &ConnectorRegistry) -> RegistrationKind {
    match &def.trigger {
        TriggerNode::Webhook(_) => RegistrationKind::Webhook,
        TriggerNode::Schedule(_) => RegistrationKind::Schedule,
        TriggerNode::Manual => RegistrationKind::Manual,
        TriggerNode::App(app) => match registry
            .get(&app.connector)
            .and_then(|c| c.manifest().trigger(&app.trigger))
            .map(|t| t.kind)
        {
            Some(TriggerKind::Polling) => RegistrationKind::Poll,
            Some(TriggerKind::Instant) | None => RegistrationKind::Instant,
        },
    }
}

fn trigger_hash(def: &FlowDefinition) -> String {
    hex::encode(Sha256::digest(
        serde_json::to_vec(&def.trigger).unwrap_or_default(),
    ))
}

/// Applies `flow.is_enabled` and the published definition (`None` = not published / deleted).
pub async fn sync_registration(
    deps: &Deps,
    flow: &Flow,
    published: Option<&FlowDefinition>,
) -> Result<(), AppError> {
    let Some(mut reg) = deps.store.get_registration_by_flow(flow.id).await? else {
        return Ok(());
    };
    let now = deps.clock.now();
    let active_def = published.filter(|_| flow.is_enabled);
    reg.is_active = active_def.is_some();
    reg.kind = published.map_or(RegistrationKind::Manual, |d| {
        registration_kind(d, &deps.connectors)
    });
    reg.next_fire_at = None;
    let hash = published.map(trigger_hash);
    if reg.state.get("trigger_hash").and_then(Value::as_str) != hash.as_deref() {
        reg.state = json!({"trigger_hash": hash});
    }
    if let Some(def) = active_def {
        match &def.trigger {
            TriggerNode::Schedule(s) => {
                reg.next_fire_at = s.schedule.next_after(now).ok().flatten();
            }
            TriggerNode::App(app) => {
                let supports_polling = deps
                    .connectors
                    .get(&app.connector)
                    .and_then(|c| {
                        c.manifest()
                            .trigger(&app.trigger)
                            .map(|t| t.supports_polling)
                    })
                    .unwrap_or(false);
                if supports_polling {
                    let default = if reg.kind == RegistrationKind::Poll {
                        DEFAULT_POLL_SECS
                    } else {
                        DEFAULT_RECONCILE_SECS
                    };
                    let secs = app
                        .poll_interval_secs
                        .map_or(default, |s| i64::from(s).max(60));
                    reg.next_fire_at = Some(now + Duration::seconds(secs));
                }
            }
            TriggerNode::Webhook(_) | TriggerNode::Manual => {}
        }
    }
    deps.store.update_registration(&reg, now).await?;
    sync_subscription(deps, &reg, active_def, hash.as_deref()).await
}

async fn sync_subscription(
    deps: &Deps,
    reg: &TriggerRegistration,
    def: Option<&FlowDefinition>,
    hash: Option<&str>,
) -> Result<(), AppError> {
    let existing = deps.store.get_subscription(reg.id).await?;
    let wanted = def.filter(|_| reg.kind == RegistrationKind::Instant);
    if let Some(sub) = &existing {
        let same = sub.data.get("trigger_hash").and_then(Value::as_str) == hash;
        if same && wanted.is_some() && sub.status == "active" {
            return Ok(());
        }
        unsubscribe(deps, reg, sub).await;
        deps.store.delete_subscription(reg.id).await?;
    }
    if let Some(def) = wanted {
        subscribe(deps, reg, def, hash).await?;
    }
    Ok(())
}

async fn unsubscribe(deps: &Deps, reg: &TriggerRegistration, sub: &Subscription) {
    let Some(connector) = deps.connectors.get(&sub.connector).cloned() else {
        return;
    };
    let Ok(connection) =
        load_connection(deps, reg.org_id, Some(&sub.connection_id.to_string())).await
    else {
        return;
    };
    let ctx = action_ctx(deps, reg.org_id, connection, String::new());
    let info = weft_connector_sdk::SubscriptionInfo {
        external_id: sub.external_id.clone(),
        expires_at: None,
        data: sub.data.clone(),
    };
    let trigger = sub
        .data
        .get("trigger")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if let Err(e) = connector.unsubscribe(&ctx, trigger, &info).await {
        tracing::warn!(registration = %reg.id, error = %e, "unsubscribe failed");
    }
}

/// Registers the app webhook (when the connector supports it). Failures are recorded and retried.
pub(crate) async fn subscribe(
    deps: &Deps,
    reg: &TriggerRegistration,
    def: &FlowDefinition,
    hash: Option<&str>,
) -> Result<(), AppError> {
    let TriggerNode::App(app) = &def.trigger else {
        return Ok(());
    };
    let Some(connector) = deps.connectors.get(&app.connector).cloned() else {
        return Ok(());
    };
    let Some(connection_id) = app
        .connection_id
        .as_deref()
        .and_then(|c| c.parse::<weft_domain::ids::ConnectionId>().ok())
    else {
        return Ok(());
    };
    let now = deps.clock.now();
    let connection = load_connection(deps, reg.org_id, Some(&connection_id.to_string()))
        .await
        .ok()
        .flatten();
    let ctx = action_ctx(deps, reg.org_id, connection, String::new());
    let channel_id = u64::from_be_bytes(
        deps.ids.uuid().as_bytes()[8..16]
            .try_into()
            .unwrap_or([0; 8]),
    ) & 0x3fff_ffff_ffff_ffff;
    let req = SubscribeRequest {
        target_url: app_event_url(deps, reg),
        verify_token: reg.secret.clone(),
        channel_id,
    };
    let mut sub = Subscription {
        id: deps.new_subscription_id(),
        org_id: reg.org_id,
        registration_id: reg.id,
        connection_id,
        connector: app.connector.clone(),
        external_id: String::new(),
        data: json!({"trigger": app.trigger, "trigger_hash": hash, "channel_id": channel_id}),
        status: "active".into(),
        expires_at: None,
        renew_after: None,
        last_error: None,
    };
    match connector
        .subscribe(&ctx, &app.trigger, &app.input, &req)
        .await
    {
        Ok(info) => {
            sub.external_id = info.external_id;
            if let Some(obj) = sub.data.as_object_mut() {
                obj.insert("provider".into(), info.data);
            }
            sub.expires_at = info
                .expires_at
                .as_deref()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|t| t.with_timezone(&chrono::Utc));
            sub.renew_after = sub.expires_at.map(|t| t - Duration::hours(24));
        }
        Err(ConnectorError::Unsupported { .. }) => return Ok(()), // manual setup (e.g. Appblocks workflow webhook)
        Err(e) => {
            tracing::warn!(registration = %reg.id, error = %e, "subscribe failed; will retry");
            sub.status = "failed".into();
            sub.last_error = Some(e.to_string());
            sub.renew_after = Some(now + Duration::minutes(5));
        }
    }
    deps.store.upsert_subscription(&sub, now).await
}
