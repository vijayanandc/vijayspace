//! Appblocks record operations and the `RecordObjects` capability.

use weft_connector_sdk::{ObjectInfo, Page, Record, RecordObjects, RecordRef, prelude::*};

use super::{
    AppblocksConnector,
    client::{Client, check_id, check_module, with_trigger},
    fields::{field_specs, module_options},
};
use crate::list_cursor::ListCursor;

/// Max page size of list/search endpoints (`openapi.yaml` `Limit`: maximum 100).
pub(super) const PAGE_LIMIT: u32 = 100;

/// Field paths in criteria: identifiers, optionally one hop (`account_id.name`).
fn check_field(field: &str) -> Result<&str, ConnectorError> {
    let ok = !field.is_empty()
        && field.split('.').count() <= 2
        && field.split('.').all(|p| check_module(p).is_ok());
    if ok {
        Ok(field)
    } else {
        Err(ConnectorError::invalid_field(
            "field",
            format!("`{field}` is not a valid field name"),
        ))
    }
}

/// An entity JSON object → `Record` (`api_id`, `modified_time`).
pub(super) fn to_record(entity: &Value) -> Option<Record> {
    let values = entity.as_object()?.clone();
    let id = values.get("api_id").and_then(Value::as_str)?.to_owned();
    let modified_at = values
        .get("modified_time")
        .and_then(Value::as_str)
        .map(str::to_owned);
    Some(Record {
        id,
        modified_at,
        values,
    })
}

pub(super) async fn modules(ctx: &ActionCtx) -> Result<Value, ConnectorError> {
    let c = Client::new(ctx).await?;
    c.send(ctx, c.request(Method::Get, "/api/modules")).await
}

pub(super) async fn fields(ctx: &ActionCtx, module: &str) -> Result<Value, ConnectorError> {
    let c = Client::new(ctx).await?;
    let module = check_module(module)?;
    c.send(
        ctx,
        c.request(Method::Get, "/api/fields")
            .query(&[("module_api_name", module)]),
    )
    .await
}

pub(super) async fn get(
    ctx: &ActionCtx,
    module: &str,
    id: &str,
) -> Result<Option<Value>, ConnectorError> {
    let c = Client::new(ctx).await?;
    let path = format!(
        "/api/{}/{}",
        check_module(module)?,
        check_id("record_id", id)?
    );
    c.send_optional(ctx, c.request(Method::Get, &path)).await
}

/// `POST /api/{module}` with a flat entity body; the response is the created entity.
pub(super) async fn create(
    ctx: &ActionCtx,
    module: &str,
    values: &Map<String, Value>,
    skip_workflows: bool,
) -> Result<Value, ConnectorError> {
    let c = Client::new(ctx).await?;
    let req = c
        .request(Method::Post, &format!("/api/{}", check_module(module)?))
        .json(&Value::Object(values.clone()));
    c.send(ctx, with_trigger(req, skip_workflows)).await
}

/// `PATCH /api/{module}/{api_id}` (partial update).
pub(super) async fn update(
    ctx: &ActionCtx,
    module: &str,
    id: &str,
    values: &Map<String, Value>,
    skip_workflows: bool,
) -> Result<Value, ConnectorError> {
    let c = Client::new(ctx).await?;
    let path = format!(
        "/api/{}/{}",
        check_module(module)?,
        check_id("record_id", id)?
    );
    let req = c
        .request(Method::Patch, &path)
        .json(&Value::Object(values.clone()));
    c.send(ctx, with_trigger(req, skip_workflows)).await
}

/// `POST /api/{module}/upsert?duplicate_check_fields=…` → `{action, api_id, data}`.
pub(super) async fn upsert(
    ctx: &ActionCtx,
    module: &str,
    values: &Map<String, Value>,
    key: &str,
    skip_workflows: bool,
) -> Result<Value, ConnectorError> {
    let c = Client::new(ctx).await?;
    let req = c
        .request(
            Method::Post,
            &format!("/api/{}/upsert", check_module(module)?),
        )
        .query(&[("duplicate_check_fields", check_field(key)?)])
        .json(&Value::Object(values.clone()));
    c.send(ctx, with_trigger(req, skip_workflows)).await
}

/// `POST /api/{module}/search` with one `eq` criterion.
pub(super) async fn search_eq(
    ctx: &ActionCtx,
    module: &str,
    field: &str,
    value: &Value,
    limit: u32,
) -> Result<Vec<Value>, ConnectorError> {
    let c = Client::new(ctx).await?;
    let body = json!({
        "criteria_pattern": "1",
        "criteria_details": [{"field_api_name": check_field(field)?, "comparator": "eq", "value": value}]
    });
    let limit = limit.clamp(1, PAGE_LIMIT).to_string();
    let req = c
        .request(
            Method::Post,
            &format!("/api/{}/search", check_module(module)?),
        )
        .query(&[("limit", &limit)])
        .json(&body);
    Ok(c.send(ctx, req)
        .await?
        .as_array()
        .cloned()
        .unwrap_or_default())
}

/// `POST /api/{parent}/{id}/{child}` with an array body (204 on success).
pub(super) async fn create_children(
    ctx: &ActionCtx,
    parent: &str,
    parent_id: &str,
    child: &str,
    rows: &[Value],
    skip_workflows: bool,
) -> Result<(), ConnectorError> {
    let c = Client::new(ctx).await?;
    let path = format!(
        "/api/{}/{}/{}",
        check_module(parent)?,
        check_id("parent_id", parent_id)?,
        check_module(child)?
    );
    let req = c
        .request(Method::Post, &path)
        .json(&Value::Array(rows.to_vec()));
    c.send(ctx, with_trigger(req, skip_workflows))
        .await
        .map(|_| ())
}

/// Records modified at or after `cursor`, oldest first (`filter[modified_time][gte]`, `sort_by`).
/// `gte` (not `gt`) so equal timestamps are never skipped; the platform dedupes by key.
pub(super) async fn modified_since(
    ctx: &ActionCtx,
    module: &str,
    cursor: Option<&str>,
    limit: u32,
    offset: u32,
) -> Result<Vec<Value>, ConnectorError> {
    let c = Client::new(ctx).await?;
    let limit = limit.clamp(1, PAGE_LIMIT).to_string();
    let offset = offset.to_string();
    let mut params = vec![
        ("sort_by", "modified_time"),
        ("sort_type", "asc"),
        ("limit", limit.as_str()),
    ];
    if offset != "0" {
        params.push(("offset", offset.as_str()));
    }
    if let Some(cursor) = cursor {
        params.push(("filter[modified_time][gte]", cursor));
    }
    let req = c
        .request(Method::Get, &format!("/api/{}", check_module(module)?))
        .query(&params);
    Ok(c.send(ctx, req)
        .await?
        .as_array()
        .cloned()
        .unwrap_or_default())
}

fn api_id(entity: &Value) -> Result<String, ConnectorError> {
    entity
        .get("api_id")
        .or_else(|| entity.get("data").and_then(|d| d.get("api_id")))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| ConnectorError::permanent("Appblocks response has no api_id"))
}

#[async_trait]
impl RecordObjects for AppblocksConnector {
    async fn list_objects(&self, ctx: &ActionCtx) -> Result<Vec<ObjectInfo>, ConnectorError> {
        Ok(module_options(&modules(ctx).await?)
            .into_iter()
            .map(|o| ObjectInfo {
                key: o.value,
                label: o.label,
            })
            .collect())
    }

    async fn describe(
        &self,
        ctx: &ActionCtx,
        object: &str,
    ) -> Result<Vec<FieldSpec>, ConnectorError> {
        Ok(field_specs(&fields(ctx, object).await?))
    }

    async fn get(
        &self,
        ctx: &ActionCtx,
        object: &str,
        id: &str,
    ) -> Result<Option<Record>, ConnectorError> {
        Ok(get(ctx, object, id).await?.as_ref().and_then(to_record))
    }

    async fn find_by(
        &self,
        ctx: &ActionCtx,
        object: &str,
        field: &str,
        value: &Value,
    ) -> Result<Vec<Record>, ConnectorError> {
        Ok(search_eq(ctx, object, field, value, 10)
            .await?
            .iter()
            .filter_map(to_record)
            .collect())
    }

    async fn create(
        &self,
        ctx: &ActionCtx,
        object: &str,
        values: &Map<String, Value>,
    ) -> Result<RecordRef, ConnectorError> {
        Ok(RecordRef {
            id: api_id(&create(ctx, object, values, true).await?)?,
        })
    }

    async fn update(
        &self,
        ctx: &ActionCtx,
        object: &str,
        id: &str,
        values: &Map<String, Value>,
    ) -> Result<RecordRef, ConnectorError> {
        update(ctx, object, id, values, true).await?;
        Ok(RecordRef { id: id.to_owned() })
    }

    async fn changed_since(
        &self,
        ctx: &ActionCtx,
        object: &str,
        cursor: Option<&Value>,
        limit: u32,
    ) -> Result<Page, ConnectorError> {
        let limit = limit.clamp(1, PAGE_LIMIT);
        let at = ListCursor::parse(cursor);
        let rows = modified_since(ctx, object, at.since.as_deref(), limit, at.skip).await?;
        let records: Vec<Record> = rows.iter().filter_map(to_record).collect();
        let full = u32::try_from(rows.len()).unwrap_or(u32::MAX) >= limit;
        let times: Vec<Option<String>> = records.iter().map(|r| r.modified_at.clone()).collect();
        // Appblocks accepts offsets up to 10,000.
        let next_cursor = full.then(|| at.next(&times, 10_000 - limit, true).to_value());
        Ok(Page {
            records,
            next_cursor,
        })
    }
}

/// Exposed for actions: the id of a created/updated entity.
pub(super) fn entity_id(entity: &Value) -> Result<String, ConnectorError> {
    api_id(entity)
}
