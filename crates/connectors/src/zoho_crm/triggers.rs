//! Zoho CRM `record_event` trigger: Notifications API channels (instant) plus polling reconciliation.

use chrono::Duration;
use weft_connector_sdk::{
    EventBatch, EventItem, InboundRequest, PollResult, SubscribeRequest, SubscriptionInfo,
    prelude::*,
};

use super::{
    client::{base, check_module, first_result, request, secure_eq, send},
    records::{PAGE_LIMIT, get_many, modified_since},
};
use crate::util::{list, opt_str};

/// Zoho allows channels for at most one week; stay a few minutes inside it.
const CHANNEL_LIFETIME_MINUTES: i64 = 7 * 24 * 60 - 10;

fn module(input: &Value) -> Result<String, ConnectorError> {
    let m = opt_str(input, "module")
        .ok_or_else(|| ConnectorError::invalid_field("module", "Choose a module"))?;
    check_module(&m)?;
    Ok(m)
}

/// `["create","edit"]` → `["Deals.create","Deals.edit"]`.
fn events(input: &Value, module: &str) -> Vec<String> {
    let mut ops = list(input, "events");
    if ops.is_empty() {
        ops = vec!["create".into(), "edit".into()];
    }
    if ops.iter().any(|o| o == "all") {
        return vec![format!("{module}.all")];
    }
    ops.iter()
        .filter(|o| matches!(o.as_str(), "create" | "edit" | "delete"))
        .map(|o| format!("{module}.{o}"))
        .collect()
}

fn watch_body(
    ctx: &ActionCtx,
    input: &Value,
    req: &SubscribeRequest,
) -> Result<(Value, String), ConnectorError> {
    let module = module(input)?;
    let expiry = (ctx.now + Duration::minutes(CHANNEL_LIFETIME_MINUTES))
        .format("%Y-%m-%dT%H:%M:%S+00:00")
        .to_string();
    if req.verify_token.len() > 50 {
        return Err(ConnectorError::invalid(
            "Zoho notification tokens are limited to 50 characters",
        ));
    }
    let body = json!({"watch": [{
        "channel_id": req.channel_id,
        "events": events(input, &module),
        "notify_url": req.target_url,
        "token": req.verify_token,
        "channel_expiry": expiry,
        "return_affected_field_values": true,
        "notify_on_related_action": false
    }]});
    Ok((body, expiry))
}

/// Checks the `watch[0]` result of an enable/update call.
fn watch_result(body: &Value) -> Result<Value, ConnectorError> {
    let wrapped = json!({"data": body.get("watch").cloned().unwrap_or(Value::Null)});
    first_result(&wrapped)
}

fn subscription(req: &SubscribeRequest, expiry: String, item: &Value) -> SubscriptionInfo {
    let returned = item
        .pointer("/details/events/0/channel_expiry")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let expires_at = returned.unwrap_or(expiry);
    let expires_at = chrono::DateTime::parse_from_rfc3339(&expires_at)
        .map_or(expires_at, |d| d.to_utc().to_rfc3339());
    SubscriptionInfo {
        external_id: req.channel_id.to_string(),
        expires_at: Some(expires_at),
        data: item.get("details").cloned().unwrap_or(Value::Null),
    }
}

/// Enables a notification channel (`POST /actions/watch`).
pub(super) async fn subscribe(
    ctx: &ActionCtx,
    input: &Value,
    req: &SubscribeRequest,
) -> Result<SubscriptionInfo, ConnectorError> {
    let (body, expiry) = watch_body(ctx, input, req)?;
    let url = format!("{}/actions/watch", base(ctx)?);
    let resp = send(ctx, |t| request(Method::Post, &url, t).json(&body)).await?;
    Ok(subscription(req, expiry, &watch_result(&resp)?))
}

/// Extends the channel expiry in place (`PATCH /actions/watch`), so no events are missed in a gap.
pub(super) async fn renew(
    ctx: &ActionCtx,
    input: &Value,
    req: &SubscribeRequest,
) -> Result<SubscriptionInfo, ConnectorError> {
    let (body, expiry) = watch_body(ctx, input, req)?;
    let url = format!("{}/actions/watch", base(ctx)?);
    let resp = send(ctx, |t| request(Method::Patch, &url, t).json(&body)).await?;
    Ok(subscription(req, expiry, &watch_result(&resp)?))
}

/// Disables the channel (`DELETE /actions/watch?channel_ids=`).
pub(super) async fn unsubscribe(
    ctx: &ActionCtx,
    sub: &SubscriptionInfo,
) -> Result<(), ConnectorError> {
    let url = format!("{}/actions/watch", base(ctx)?);
    let id = sub.external_id.clone();
    send(ctx, |t| {
        request(Method::Delete, &url, t).query(&[("channel_ids", &id)])
    })
    .await
    .map(|_| ())
}

/// Zoho operation names → the trigger's `events` vocabulary.
fn op_name(operation: &str) -> &str {
    match operation {
        "insert" => "create",
        "update" => "edit",
        other => other,
    }
}

/// Verifies the channel token, then fetches the notified records (Zoho sends ids only).
pub(super) async fn handle_event(
    ctx: &ActionCtx,
    input: &Value,
    request: &InboundRequest,
    verify_token: &str,
) -> Result<EventBatch, ConnectorError> {
    let body = &request.body;
    let token = body
        .get("token")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if verify_token.is_empty() || !secure_eq(token, verify_token) {
        return Err(ConnectorError::invalid(
            "webhook.unauthorized: notification token mismatch",
        ));
    }
    let module = opt_str(body, "module")
        .map_or_else(|| module(input), |m| check_module(&m).map(str::to_owned))?;
    let operation = op_name(
        body.get("operation")
            .and_then(Value::as_str)
            .unwrap_or("update"),
    )
    .to_owned();
    let wanted = events(input, &module);
    let exact = format!("{module}.{operation}");
    let all = format!("{module}.all");
    if !wanted.iter().any(|e| *e == all || *e == exact) {
        return Ok(EventBatch {
            items: Vec::new(),
            response: None,
        });
    }
    let ids: Vec<String> = body
        .get("ids")
        .and_then(Value::as_array)
        .map(|ids| {
            ids.iter()
                .filter_map(|v| {
                    v.as_str()
                        .map(str::to_owned)
                        .or_else(|| v.as_u64().map(|n| n.to_string()))
                })
                .collect()
        })
        .unwrap_or_default();
    let affected = body.get("affected_fields").cloned().unwrap_or(Value::Null);
    if operation == "delete" {
        let items = ids
            .into_iter()
            .map(|id| EventItem {
                dedupe_key: format!("{id}:deleted"),
                record_id: Some(id.clone()),
                data: json!({"module": module, "operation": operation, "id": id, "record": null}),
            })
            .collect();
        return Ok(EventBatch {
            items,
            response: None,
        });
    }
    let records = if ids.is_empty() {
        Vec::new()
    } else {
        get_many(ctx, &module, &ids).await?
    };
    let items = records
        .into_iter()
        .filter_map(|rec| item(&module, &operation, rec, &affected))
        .collect();
    Ok(EventBatch {
        items,
        response: None,
    })
}

fn item(module: &str, operation: &str, rec: Value, affected: &Value) -> Option<EventItem> {
    let id = rec.get("id").and_then(|v| {
        v.as_str()
            .map(str::to_owned)
            .or_else(|| v.as_u64().map(|n| n.to_string()))
    })?;
    let modified = rec
        .get("Modified_Time")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    Some(EventItem {
        dedupe_key: format!("{id}:{modified}"),
        record_id: Some(id.clone()),
        data: json!({"module": module, "operation": operation, "id": id, "record": rec, "affected_fields": affected}),
    })
}

/// Reconciliation / polling: records modified since the cursor. The first poll is a baseline.
pub(super) async fn poll(
    ctx: &ActionCtx,
    input: &Value,
    cursor: Option<&Value>,
) -> Result<PollResult, ConnectorError> {
    let module = module(input)?;
    let Some(cursor) = cursor.and_then(Value::as_str) else {
        return Ok(PollResult {
            items: Vec::new(),
            cursor: Some(Value::String(
                ctx.now.format("%Y-%m-%dT%H:%M:%S+00:00").to_string(),
            )),
        });
    };
    let rows = modified_since(ctx, &module, Some(cursor), PAGE_LIMIT).await?;
    let mut next = cursor.to_owned();
    let items: Vec<EventItem> = rows
        .into_iter()
        .filter_map(|rec| {
            if let Some(m) = rec.get("Modified_Time").and_then(Value::as_str) {
                m.clone_into(&mut next);
            }
            item(&module, "edit", rec, &Value::Null)
        })
        .collect();
    Ok(PollResult {
        items,
        cursor: Some(Value::String(next)),
    })
}
