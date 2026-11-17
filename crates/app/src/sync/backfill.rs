//! Backfill: start one execution per existing record of an app trigger's module (initial sync).

use chrono::SecondsFormat;
use serde_json::{Value, json};
use weft_connector_sdk::EventItem;
use weft_domain::{
    error_codes,
    flow::TriggerNode,
    ids::{FlowId, OrgId},
    model::ExecutionMode,
};

use crate::{
    AppError, Deps,
    runner::action::{action_ctx, load_connection},
    triggers::ingress_items,
};

/// Pages through the trigger module's records (up to `limit`) and starts a backfill execution for each.
pub async fn backfill(
    deps: &Deps,
    org: OrgId,
    flow_id: FlowId,
    limit: u32,
) -> Result<usize, AppError> {
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
    let connection = load_connection(deps, org, app.connection_id.as_deref())
        .await
        .map_err(|e| AppError::validation(error_codes::CONNECTION_NOT_FOUND, e.to_string()))?;
    let ctx = action_ctx(deps, org, connection, String::new());
    let reg = deps
        .store
        .get_registration_by_flow(flow_id)
        .await?
        .ok_or_else(|| AppError::internal("registration missing"))?;
    let mut cursor: Option<Value> = None;
    let mut total = 0usize;
    let started = deps.clock.now().to_rfc3339_opts(SecondsFormat::Secs, true);
    while total < limit as usize {
        let page = records
            .changed_since(&ctx, module, cursor.as_ref(), 100)
            .await
            .map_err(|e| AppError::new(crate::ErrorKind::BadGateway, e.code(), e.to_string()))?;
        let items: Vec<EventItem> = page
            .records
            .into_iter()
            .take(limit as usize - total)
            .map(|r| EventItem {
                dedupe_key: format!("backfill:{started}:{}", r.id),
                record_id: Some(r.id.clone()),
                data: json!({"module": module, "operation": "backfill", "id": r.id, "record": Value::Object(r.values)}),
            })
            .collect();
        if items.is_empty() {
            break;
        }
        total += items.len();
        ingress_items(deps, &reg, &flow, &version, &items, ExecutionMode::Backfill).await?;
        match page.next_cursor {
            Some(c) => cursor = Some(c),
            None => break,
        }
    }
    Ok(total)
}
