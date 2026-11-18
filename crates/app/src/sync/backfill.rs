//! Backfill: start one execution per existing record of an app trigger's module (initial sync).

use chrono::SecondsFormat;
use serde_json::{Value, json};
use weft_connector_sdk::EventItem;
use weft_domain::{error_codes, flow::TriggerNode, ids::FlowId, model::ExecutionMode};

use crate::tenancy::Actor;
use crate::{
    AppError, Deps,
    limits::RequestClass,
    runner::action::{action_ctx_for, load_connection},
    triggers::ingress_items,
};
use weft_domain::permissions::Permission;

/// Upper bound for one backfill call.
pub const MAX_BACKFILL: u32 = 2_000;

/// What one backfill call did.
#[derive(Debug, Clone, PartialEq)]
pub struct BackfillPage {
    pub enqueued: usize,
    /// Pass back as `cursor` to continue; `None` when every record has been started.
    pub next_cursor: Option<Value>,
}

/// Pages through the trigger module's records from `cursor` (whole pages, about `limit` records,
/// at most [`MAX_BACKFILL`]) and starts a backfill execution for each.
pub async fn backfill(
    deps: &Deps,
    actor: &Actor,
    flow_id: FlowId,
    limit: u32,
    cursor: Option<Value>,
) -> Result<BackfillPage, AppError> {
    actor.require(Permission::FlowEdit)?;
    let org = actor.org;
    // Runs inside the HTTP request: one call is bounded, and `next_cursor` continues it.
    let limit = limit.clamp(1, MAX_BACKFILL);
    let flow = deps
        .store
        .get_flow(org, flow_id)
        .await?
        .ok_or_else(|| AppError::not_found(error_codes::FLOW_NOT_FOUND, "Flow not found"))?;
    let version_id = flow.published_version_id.ok_or_else(|| {
        AppError::conflict(error_codes::FLOW_NOT_PUBLISHED, "Publish the flow first")
    })?;
    let version = deps
        .store
        .get_version(version_id)
        .await?
        .ok_or_else(|| AppError::internal("published version missing"))?;
    let TriggerNode::App(app) = &version.definition.trigger else {
        return Err(AppError::validation(
            error_codes::VALIDATION_FAILED,
            "Backfill needs an app trigger",
        ));
    };
    let connector = deps
        .connectors
        .get(&app.connector)
        .cloned()
        .ok_or_else(|| AppError::not_found(error_codes::CONNECTOR_NOT_FOUND, "Unknown app"))?;
    let records = connector.records().ok_or_else(|| {
        AppError::validation(
            error_codes::VALIDATION_FAILED,
            "This app does not support backfill",
        )
    })?;
    let module = app
        .input
        .get("module")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            AppError::validation(error_codes::VALIDATION_FAILED, "The trigger has no module")
        })?;
    let connection = load_connection(
        deps,
        org,
        app.connection_id.as_deref(),
        Some(&app.connector),
    )
    .await
    .map_err(|e| AppError::validation(error_codes::CONNECTION_NOT_FOUND, e.to_string()))?;
    let ctx = action_ctx_for(
        deps,
        org,
        connection,
        String::new(),
        RequestClass::Background,
    );
    let reg = deps
        .store
        .get_registration_by_flow(flow_id)
        .await?
        .ok_or_else(|| AppError::internal("registration missing"))?;
    let mut cursor = cursor.filter(|c| !c.is_null());
    let mut total = 0usize;
    let started = deps.clock.now().to_rfc3339_opts(SecondsFormat::Secs, true);
    while total < limit as usize {
        let page = records
            .changed_since(&ctx, module, cursor.as_ref(), 100)
            .await
            .map_err(|e| AppError::new(crate::ErrorKind::BadGateway, e.code(), e.to_string()))?;
        // Whole pages only, so `next_cursor` never skips records.
        let items: Vec<EventItem> = page
            .records
            .into_iter()
            .map(|r| EventItem {
                dedupe_key: format!("backfill:{started}:{}", r.id),
                record_id: Some(r.id.clone()),
                data: json!({"module": module, "operation": "backfill", "id": r.id, "record": Value::Object(r.values)}),
            })
            .collect();
        total += items.len();
        if !items.is_empty() {
            ingress_items(deps, &reg, &flow, &version, &items, ExecutionMode::Backfill).await?;
        }
        if page.next_cursor.is_some() && page.next_cursor == cursor {
            // A connector whose cursor does not move would make callers loop forever.
            return Err(AppError::new(
                crate::ErrorKind::BadGateway,
                error_codes::BACKFILL_STALLED,
                "The app returned the same page again; backfill cannot continue",
            ));
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    Ok(BackfillPage {
        enqueued: total,
        next_cursor: cursor,
    })
}
