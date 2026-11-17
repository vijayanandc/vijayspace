//! Appblocks triggers: workflow-webhook events (instant, set up by an admin) and polling.

use weft_connector_sdk::{EventBatch, EventItem, InboundRequest, PollResult, prelude::*};

use super::{
    client::{check_id, check_module, secure_eq},
    records::{PAGE_LIMIT, get, modified_since},
};
use crate::util::opt_str;

/// Header that must carry the trigger's secret (configured as a static workflow-webhook header).
pub(super) const KEY_HEADER: &str = "x-weft-key";

fn operation_matches(filter: Option<&str>, operation: &str) -> bool {
    match filter {
        None | Some("" | "any") => true,
        Some(f) => f == operation,
    }
}

/// Handles a workflow-webhook delivery: `{"module", "operation", "api_id"}`.
/// Verifies the secret header, then fetches the full record (consistent payloads whatever the template).
pub(super) async fn handle_event(
    ctx: &ActionCtx,
    input: &Value,
    request: &InboundRequest,
    verify_token: &str,
) -> Result<EventBatch, ConnectorError> {
    let presented = request.header(KEY_HEADER).unwrap_or_default();
    if verify_token.is_empty() || !secure_eq(presented, verify_token) {
        return Err(ConnectorError::invalid(
            "webhook.unauthorized: missing or wrong X-Weft-Key header",
        ));
    }
    let body = &request.body;
    let module = opt_str(body, "module")
        .or_else(|| opt_str(input, "module"))
        .ok_or_else(|| ConnectorError::invalid_field("module", "The event has no module"))?;
    check_module(&module)?;
    let operation = opt_str(body, "operation").unwrap_or_else(|| "update".into());
    if !operation_matches(opt_str(input, "operation").as_deref(), &operation) {
        return Ok(EventBatch {
            items: Vec::new(),
            response: None,
        });
    }
    let api_id = check_id(
        "api_id",
        &opt_str(body, "api_id")
            .ok_or_else(|| ConnectorError::invalid_field("api_id", "The event has no api_id"))?,
    )?;
    let delivery_key = request
        .header("idempotency-key")
        .or_else(|| request.header("x-appblocks-event-id"))
        .map(str::to_owned);
    let record = if operation == "delete" {
        None
    } else {
        get(ctx, &module, &api_id).await?
    };
    let modified = record
        .as_ref()
        .and_then(|r| r.get("modified_time"))
        .and_then(Value::as_str)
        .filter(|m| !m.is_empty());
    // The record version (`{api_id}:{modified_time}`) is the key whenever there is one: polling
    // builds the same key, so a version delivered live is not run again by the reconciliation
    // poll. The delivery ID is only a fallback (deletes, records without a modified time).
    let dedupe_key = match (modified, delivery_key) {
        (Some(m), _) => format!("{api_id}:{m}"),
        (None, Some(delivery)) => delivery,
        (None, None) => format!("{api_id}:deleted"),
    };
    let item = EventItem {
        dedupe_key,
        record_id: Some(api_id.clone()),
        data: json!({"module": module, "operation": operation, "id": api_id, "record": record}),
    };
    Ok(EventBatch {
        items: vec![item],
        response: None,
    })
}

/// Polls records modified since the cursor. The first poll is a baseline (no items).
pub(super) async fn poll(
    ctx: &ActionCtx,
    input: &Value,
    cursor: Option<&Value>,
) -> Result<PollResult, ConnectorError> {
    let module = opt_str(input, "module")
        .ok_or_else(|| ConnectorError::invalid_field("module", "Choose a module"))?;
    let Some(cursor) = cursor.and_then(Value::as_str) else {
        return Ok(PollResult {
            items: Vec::new(),
            cursor: Some(Value::String(ctx.now.to_rfc3339())),
        });
    };
    let rows = modified_since(ctx, &module, Some(cursor), PAGE_LIMIT).await?;
    let mut next = cursor.to_owned();
    let items = rows
        .into_iter()
        .filter_map(|record| {
            let id = record.get("api_id").and_then(Value::as_str)?.to_owned();
            let modified = record
                .get("modified_time")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            if !modified.is_empty() {
                next.clone_from(&modified);
            }
            Some(EventItem {
                dedupe_key: format!("{id}:{modified}"),
                record_id: Some(id.clone()),
                data: json!({"module": module, "operation": "update", "id": id, "record": record}),
            })
        })
        .collect();
    Ok(PollResult {
        items,
        cursor: Some(Value::String(next)),
    })
}

/// Workflow setup instructions shown in the builder.
pub(super) const SETUP: &str = "In Appblocks, open Settings → Workflows and create a workflow on the chosen module \
(trigger: create / update / delete as needed). Add a Webhook action:\n\
• URL: this flow's App event URL (shown above)\n\
• Method: POST\n\
• Header: X-Weft-Key = the secret shown above\n\
• Payload: {\"module\": \"<module>\", \"operation\": \"<create|update|delete>\", \"api_id\": \"{api_id}\"}\n\
Weft fetches the full record itself, so the payload only needs these three keys. \
Writes Weft makes into Appblocks skip workflows (?trigger=), so they never loop back.";
