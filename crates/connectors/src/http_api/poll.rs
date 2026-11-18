//! The polling triggers of "Any app". Both read a list endpoint and remember up to `SEEN_MAX` items between polls;
//! the first poll only remembers what is there, so runs start for what happens after the flow is switched on.
//! - "New items": a run per item whose ID was not seen before.
//! - "New or changed items": also a run when an item's version changes: a field that changes with it (e.g.
//!   `last_modified_time`), or, without one, the item's content. Only the items the endpoint returns are compared,
//!   so sort or filter the list so recent changes come first.

use sha2::{Digest, Sha256};
use weft_connector_sdk::{EventItem, PollResult, prelude::*};

use super::{HttpApiConnector, auth, base_url};
use crate::util::{key_values, opt_str, req_str};

/// Items remembered between polls (newest first).
const SEEN_MAX: usize = 1000;

/// The list in `body` at a dotted path (`data.items`; empty = the body itself).
pub(super) fn items_at<'a>(body: &'a Value, path: &str) -> Option<&'a Vec<Value>> {
    path.split('.')
        .filter(|p| !p.is_empty())
        .try_fold(body, |v, key| v.get(key))?
        .as_array()
}

/// An item's ID as text (a string or a number).
pub(super) fn item_id(item: &Value, field: &str) -> Option<String> {
    let value = item.get(field)?;
    value
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .or_else(|| value.as_number().map(ToString::to_string))
}

/// `(id, item)` for every item of the list the trigger's endpoint returns.
async fn fetch(
    app: &HttpApiConnector,
    ctx: &ActionCtx,
    input: &Value,
) -> Result<Vec<(String, Value)>, ConnectorError> {
    let url = auth::url_for(&base_url(ctx)?, &req_str(input, "path")?)?;
    let resp = app
        .send(ctx, || {
            let query = key_values(input, "query");
            let refs: Vec<(&str, &str)> = query
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();
            Ok(HttpRequest::new(Method::Get, url.clone())
                .query(&refs)
                .timeout(ctx.timeout))
        })
        .await?;
    if !resp.is_success() {
        return Err(ConnectorError::from_http(&resp));
    }
    let body = resp.json()?;
    let items =
        items_at(&body, &opt_str(input, "items_path").unwrap_or_default()).ok_or_else(|| {
            ConnectorError::invalid_field("items_path", "The answer has no list at this path")
        })?;
    let id_field = opt_str(input, "id_field").unwrap_or_else(|| "id".into());
    Ok(items
        .iter()
        .filter_map(|i| item_id(i, &id_field).map(|id| (id, i.clone())))
        .collect())
}

pub(super) async fn new_items(
    app: &HttpApiConnector,
    ctx: &ActionCtx,
    input: &Value,
    cursor: Option<&Value>,
) -> Result<PollResult, ConnectorError> {
    let ids = fetch(app, ctx, input).await?;
    let seen: Vec<String> = cursor
        .and_then(|c| c.get("seen"))
        .and_then(Value::as_array)
        .map(|ids| {
            ids.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let new: Vec<EventItem> = if cursor.is_none() {
        Vec::new()
    } else {
        ids.iter()
            .filter(|(id, _)| !seen.contains(id))
            .map(|(id, item)| EventItem {
                dedupe_key: format!("item:{id}"),
                record_id: Some(id.clone()),
                data: json!({"id": id, "item": item}),
            })
            .collect()
    };
    let mut remembered: Vec<String> = ids.into_iter().map(|(id, _)| id).collect();
    let older: Vec<String> = seen
        .into_iter()
        .filter(|s| !remembered.contains(s))
        .collect();
    remembered.extend(older);
    remembered.truncate(SEEN_MAX);
    Ok(PollResult {
        items: new,
        cursor: Some(json!({"seen": remembered})),
    })
}

/// An item's version: its version field as text, or a hash of the whole item.
fn version_of(item: &Value, field: Option<&str>) -> String {
    let short = |text: &str| hex::encode(&Sha256::digest(text.as_bytes())[..12]);
    match field.and_then(|f| item.get(f)) {
        Some(Value::String(s)) if s.len() <= 64 => s.clone(),
        Some(v @ (Value::Number(_) | Value::Bool(_))) => v.to_string(),
        Some(v) => short(&v.to_string()),
        None => short(&item.to_string()),
    }
}

pub(super) async fn changed_items(
    app: &HttpApiConnector,
    ctx: &ActionCtx,
    input: &Value,
    cursor: Option<&Value>,
) -> Result<PollResult, ConnectorError> {
    let field = opt_str(input, "version_field");
    let current: Vec<(String, String, Value)> = fetch(app, ctx, input)
        .await?
        .into_iter()
        .map(|(id, item)| {
            let version = version_of(&item, field.as_deref());
            (id, version, item)
        })
        .collect();
    let known: Vec<(String, String)> = cursor
        .and_then(|c| c.get("versions"))
        .and_then(Value::as_array)
        .map(|pairs| {
            pairs
                .iter()
                .filter_map(|p| {
                    Some((
                        p.get(0)?.as_str()?.to_owned(),
                        p.get(1)?.as_str()?.to_owned(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    let before = |id: &str| known.iter().find(|(k, _)| k == id).map(|(_, v)| v.as_str());
    let items: Vec<EventItem> = if cursor.is_none() {
        Vec::new()
    } else {
        current
            .iter()
            .filter_map(|(id, version, item)| {
                let change = match before(id) {
                    None => "new",
                    Some(v) if v != version => "changed",
                    Some(_) => return None,
                };
                Some(EventItem {
                    dedupe_key: format!("item:{id}:{version}"),
                    record_id: Some(id.clone()),
                    data: json!({"id": id, "change": change, "item": item}),
                })
            })
            .collect()
    };
    let mut remembered: Vec<Value> = current.iter().map(|(id, v, _)| json!([id, v])).collect();
    remembered.extend(
        known
            .iter()
            .filter(|(id, _)| !current.iter().any(|(c, _, _)| c == id))
            .map(|(id, v)| json!([id, v])),
    );
    remembered.truncate(SEEN_MAX);
    Ok(PollResult {
        items,
        cursor: Some(json!({"versions": remembered})),
    })
}
