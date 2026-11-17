//! Zoho CRM record operations and the `RecordObjects` capability.

use weft_connector_sdk::{ObjectInfo, Page, Record, RecordObjects, RecordRef, prelude::*};

use super::{
    ZohoCrmConnector,
    client::{base, check_id, check_module, first_result, request, send},
    fields::{field_specs, list_field_names, module_options},
};
use crate::list_cursor::ListCursor;

/// Zoho list/search page size limit.
pub(super) const PAGE_LIMIT: u32 = 200;

pub(super) async fn modules(ctx: &ActionCtx) -> Result<Value, ConnectorError> {
    let url = format!("{}/settings/modules", base(ctx)?);
    send(ctx, |t| request(Method::Get, &url, t)).await
}

pub(super) async fn fields(ctx: &ActionCtx, module: &str) -> Result<Value, ConnectorError> {
    let url = format!("{}/settings/fields", base(ctx)?);
    let module = check_module(module)?.to_owned();
    send(ctx, |t| {
        request(Method::Get, &url, t).query(&[("module", &module)])
    })
    .await
}

/// Write body: `{"data": [values]}` plus `"trigger": []` when Zoho automation must not run.
fn write_body(values: &Map<String, Value>, trigger_workflows: bool) -> Map<String, Value> {
    let mut body = Map::new();
    body.insert("data".into(), json!([values]));
    if !trigger_workflows {
        body.insert("trigger".into(), json!([]));
    }
    body
}

pub(super) async fn create(
    ctx: &ActionCtx,
    module: &str,
    values: &Map<String, Value>,
    trigger_workflows: bool,
) -> Result<Value, ConnectorError> {
    let url = format!("{}/{}", base(ctx)?, check_module(module)?);
    let body = Value::Object(write_body(values, trigger_workflows));
    first_result(&send(ctx, |t| request(Method::Post, &url, t).json(&body)).await?)
}

pub(super) async fn update(
    ctx: &ActionCtx,
    module: &str,
    id: &str,
    values: &Map<String, Value>,
    trigger_workflows: bool,
) -> Result<Value, ConnectorError> {
    let url = format!(
        "{}/{}/{}",
        base(ctx)?,
        check_module(module)?,
        check_id("record_id", id)?
    );
    let body = Value::Object(write_body(values, trigger_workflows));
    first_result(&send(ctx, |t| request(Method::Put, &url, t).json(&body)).await?)
}

pub(super) async fn upsert(
    ctx: &ActionCtx,
    module: &str,
    values: &Map<String, Value>,
    duplicate_check_fields: &[String],
    trigger_workflows: bool,
) -> Result<Value, ConnectorError> {
    let url = format!("{}/{}/upsert", base(ctx)?, check_module(module)?);
    let mut body = write_body(values, trigger_workflows);
    if !duplicate_check_fields.is_empty() {
        body.insert(
            "duplicate_check_fields".into(),
            json!(duplicate_check_fields),
        );
    }
    let body = Value::Object(body);
    first_result(&send(ctx, |t| request(Method::Post, &url, t).json(&body)).await?)
}

/// One record, or `None` when it does not exist.
pub(super) async fn get(
    ctx: &ActionCtx,
    module: &str,
    id: &str,
) -> Result<Option<Value>, ConnectorError> {
    let url = format!(
        "{}/{}/{}",
        base(ctx)?,
        check_module(module)?,
        check_id("record_id", id)?
    );
    match send(ctx, |t| request(Method::Get, &url, t)).await {
        Ok(body) => Ok(body
            .get("data")
            .and_then(Value::as_array)
            .and_then(|d| d.first())
            .cloned()),
        Err(ConnectorError::NotFound { .. }) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Escapes `(`, `)` and `,` in criteria values, as Zoho requires.
fn escape_criteria(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
        .replace(',', "\\,")
}

/// `(field:equals:value)`.
pub(super) fn criteria_eq(field: &str, value: &Value) -> Result<String, ConnectorError> {
    let field = check_module(field).map_err(|_| {
        ConnectorError::invalid_field("field", format!("`{field}` is not a valid field name"))
    })?;
    let text = value
        .as_str()
        .map_or_else(|| value.to_string(), str::to_owned);
    Ok(format!("({field}:equals:{})", escape_criteria(&text)))
}

/// `GET /{module}/search?criteria=…` (204 → empty).
pub(super) async fn search(
    ctx: &ActionCtx,
    module: &str,
    criteria: &str,
    limit: u32,
) -> Result<Vec<Value>, ConnectorError> {
    let url = format!("{}/{}/search", base(ctx)?, check_module(module)?);
    let per_page = limit.clamp(1, PAGE_LIMIT).to_string();
    let body = send(ctx, |t| {
        request(Method::Get, &url, t).query(&[("criteria", criteria), ("per_page", &per_page)])
    })
    .await?;
    Ok(body
        .get("data")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

/// Several records by id (`GET /{module}?ids=…`, chunks of 100).
pub(super) async fn get_many(
    ctx: &ActionCtx,
    module: &str,
    ids: &[String],
) -> Result<Vec<Value>, ConnectorError> {
    let url = format!("{}/{}", base(ctx)?, check_module(module)?);
    let names = list_field_names(&field_specs(&fields(ctx, module).await?));
    let mut out = Vec::new();
    for chunk in ids.chunks(100) {
        let joined = chunk
            .iter()
            .map(|id| check_id("ids", id))
            .collect::<Result<Vec<_>, _>>()?
            .join(",");
        let body = send(ctx, |t| {
            request(Method::Get, &url, t).query(&[("ids", &joined), ("fields", &names)])
        })
        .await?;
        out.extend(
            body.get("data")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
        );
    }
    Ok(out)
}

/// Records modified after `since` (or from the beginning), oldest first.
pub(super) async fn modified_since(
    ctx: &ActionCtx,
    module: &str,
    since: Option<&str>,
    limit: u32,
    page: u32,
) -> Result<Vec<Value>, ConnectorError> {
    let url = format!("{}/{}", base(ctx)?, check_module(module)?);
    let names = list_field_names(&field_specs(&fields(ctx, module).await?));
    let per_page = limit.clamp(1, PAGE_LIMIT).to_string();
    let page = page.max(1).to_string();
    let since = since.map(str::to_owned);
    let body = send(ctx, |t| {
        let req = request(Method::Get, &url, t).query(&[
            ("fields", &names),
            ("sort_by", "Modified_Time"),
            ("sort_order", "asc"),
            ("per_page", &per_page),
            ("page", &page),
        ]);
        match &since {
            Some(s) => req.header("If-Modified-Since", s.clone()),
            None => req,
        }
    })
    .await?;
    Ok(body
        .get("data")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

/// `POST /{module}/{id}/Notes`.
pub(super) async fn add_note(
    ctx: &ActionCtx,
    module: &str,
    id: &str,
    title: &str,
    content: &str,
) -> Result<Value, ConnectorError> {
    let url = format!(
        "{}/{}/{}/Notes",
        base(ctx)?,
        check_module(module)?,
        check_id("record_id", id)?
    );
    let body = json!({"data": [{"Note_Title": title, "Note_Content": content}]});
    first_result(&send(ctx, |t| request(Method::Post, &url, t).json(&body)).await?)
}

/// `POST /Tasks` related to a record (`What_Id` + `$se_module`).
pub(super) async fn create_task(
    ctx: &ActionCtx,
    task: &Map<String, Value>,
) -> Result<Value, ConnectorError> {
    let url = format!("{}/Tasks", base(ctx)?);
    let body = json!({"data": [task], "trigger": []});
    first_result(&send(ctx, |t| request(Method::Post, &url, t).json(&body)).await?)
}

/// A Zoho record JSON → `Record`.
pub(super) fn to_record(rec: &Value) -> Option<Record> {
    let values = rec.as_object()?.clone();
    let id = match values.get("id")? {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Null | Value::Bool(_) | Value::Array(_) | Value::Object(_) => return None,
    };
    let modified_at = values
        .get("Modified_Time")
        .and_then(Value::as_str)
        .map(str::to_owned);
    Some(Record {
        id,
        modified_at,
        values,
    })
}

/// The record id inside a write result (`details.id`).
pub(super) fn result_id(item: &Value) -> Result<String, ConnectorError> {
    item.get("details")
        .and_then(|d| d.get("id"))
        .and_then(|v| {
            v.as_str()
                .map(str::to_owned)
                .or_else(|| v.as_u64().map(|n| n.to_string()))
        })
        .ok_or_else(|| ConnectorError::permanent(format!("Zoho response has no record id: {item}")))
}

#[async_trait]
impl RecordObjects for ZohoCrmConnector {
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
        Ok(search(ctx, object, &criteria_eq(field, value)?, 10)
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
            id: result_id(&create(ctx, object, values, false).await?)?,
        })
    }

    async fn update(
        &self,
        ctx: &ActionCtx,
        object: &str,
        id: &str,
        values: &Map<String, Value>,
    ) -> Result<RecordRef, ConnectorError> {
        update(ctx, object, id, values, false).await?;
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
        let page = at.skip / limit + 1;
        let rows = modified_since(ctx, object, at.since.as_deref(), limit, page).await?;
        let records: Vec<Record> = rows.iter().filter_map(to_record).collect();
        let full = u32::try_from(rows.len()).unwrap_or(u32::MAX) >= limit;
        let times: Vec<Option<String>> = records.iter().map(|r| r.modified_at.clone()).collect();
        // Re-anchor on every page (page numbers only inside a group of equal times): a record
        // edited between pages moves to the end, and a plain page number would skip unread ones.
        // Records at the anchor time are read again and deduplicated downstream.
        let next_cursor = full.then(|| at.next(&times, 0, false).to_value());
        Ok(Page {
            records,
            next_cursor,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn criteria_escaping() {
        assert_eq!(
            criteria_eq("Email", &json!("a@b.com")).unwrap(),
            "(Email:equals:a@b.com)"
        );
        assert_eq!(
            criteria_eq("Account_Name", &json!("Acme (India), Ltd")).unwrap(),
            "(Account_Name:equals:Acme \\(India\\)\\, Ltd)"
        );
        assert!(criteria_eq("Email)or(x", &json!("v")).is_err());
    }
}
