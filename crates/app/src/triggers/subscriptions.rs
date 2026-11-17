//! Renewing provider subscriptions before they expire (Zoho channels last at most a week)
//! and retrying failed subscriptions.

use chrono::Duration;
use weft_connector_sdk::{SubscribeRequest, SubscriptionInfo};
use weft_domain::flow::TriggerNode;

use crate::{
    AppError, Deps,
    flows::manage_urls::app_event_url,
    runner::action::{action_ctx, load_connection},
};

/// Renews or retries due subscriptions; returns how many were processed.
pub async fn renew_due_subscriptions(deps: &Deps) -> Result<usize, AppError> {
    let now = deps.clock.now();
    let due = deps
        .store
        .claim_due_subscriptions(now, now + Duration::minutes(10), 20)
        .await?;
    let mut n = 0;
    for sub in due {
        n += 1;
        let id = sub.id;
        // One failing subscription (or a database error) must not hold up the rest of the batch.
        if let Err(e) = renew_one(deps, sub, now).await {
            tracing::warn!(subscription = %id, error = %e, "subscription renewal step failed");
        }
    }
    Ok(n)
}

/// Renews (or re-subscribes) one claimed subscription and stores the outcome.
async fn renew_one(
    deps: &Deps,
    mut sub: crate::ports::Subscription,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<(), AppError> {
    let Some(reg) = deps.store.get_registration(sub.registration_id).await? else {
        deps.store.delete_subscription(sub.registration_id).await?;
        return Ok(());
    };
    let flow = deps.store.get_flow_system(reg.flow_id).await?;
    let version = match flow.and_then(|f| f.published_version_id) {
        Some(v) => deps.store.get_version(v).await?,
        None => None,
    };
    let Some(TriggerNode::App(app)) = version.map(|v| v.definition.trigger) else {
        deps.store.delete_subscription(sub.registration_id).await?;
        return Ok(());
    };
    let Some(connector) = deps.connectors.get(&sub.connector).cloned() else {
        return Ok(());
    };
    let connection = load_connection(deps, reg.org_id, Some(&sub.connection_id.to_string()))
        .await
        .ok()
        .flatten();
    let ctx = action_ctx(deps, reg.org_id, connection, String::new());
    let channel_id = sub
        .data
        .get("channel_id")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let req = SubscribeRequest {
        target_url: app_event_url(deps, &reg),
        verify_token: reg.secret.clone(),
        channel_id,
    };
    let current = SubscriptionInfo {
        external_id: sub.external_id.clone(),
        expires_at: None,
        data: sub.data.get("provider").cloned().unwrap_or_default(),
    };
    let result = if sub.status == "failed" || sub.external_id.is_empty() {
        connector
            .subscribe(&ctx, &app.trigger, &app.input, &req)
            .await
    } else {
        connector
            .renew(&ctx, &app.trigger, &app.input, &current, &req)
            .await
    };
    match result {
        Ok(info) => {
            sub.external_id = info.external_id;
            sub.status = "active".into();
            sub.last_error = None;
            sub.expires_at = info
                .expires_at
                .as_deref()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|t| t.with_timezone(&chrono::Utc));
            sub.renew_after = sub.expires_at.map(|t| t - Duration::hours(24));
        }
        Err(e) => {
            tracing::warn!(subscription = %sub.id, error = %e, "subscription renewal failed");
            sub.status = "failed".into();
            sub.last_error = Some(e.to_string());
            sub.renew_after = Some(now + Duration::minutes(15));
        }
    }
    deps.store.upsert_subscription(&sub, now).await?;
    Ok(())
}
