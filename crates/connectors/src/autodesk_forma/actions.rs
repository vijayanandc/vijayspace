//! Forma actions: projects, members, issues, RFIs.

use weft_connector_sdk::prelude::*;

use super::client::{Forma, account_id, plain_id, results, seg};
use crate::util::{list, opt_f64, opt_str, req_str};

/// Dispatches one action.
pub(crate) async fn run(
    ctx: &ActionCtx,
    action: &str,
    input: &Value,
) -> Result<Value, ConnectorError> {
    match action {
        "create_project" => create_project(ctx, input).await,
        "add_project_user" => add_project_user(ctx, input).await,
        "create_issue" => create_issue(ctx, input).await,
        "update_issue" => update_issue(ctx, input).await,
        "get_issue" => get_issue(ctx, input).await,
        "list_issues" => list_issues(ctx, input).await,
        "create_rfi" => create_rfi(ctx, input).await,
        "get_project" => get_project(ctx, input).await,
        other => Err(ConnectorError::unsupported(other)),
    }
}

fn project_id(input: &Value) -> Result<String, ConnectorError> {
    req_str(input, "project_id").map(|p| plain_id(&p).to_owned())
}

/// `YYYY-MM-DD` from a date or date-time string.
fn date_only(input: &Value, key: &str) -> Option<String> {
    let text = opt_str(input, key)?;
    Some(weft_expr::parse_datetime(&text).map_or(text, |d| d.format("%Y-%m-%d").to_string()))
}

/// Copies optional string inputs into a JSON body under (possibly renamed) keys.
fn copy_strings(input: &Value, body: &mut Map<String, Value>, pairs: &[(&str, &str)]) {
    for (src, dst) in pairs {
        if let Some(v) = opt_str(input, src) {
            body.insert((*dst).to_owned(), Value::String(v));
        }
    }
}

async fn create_project(ctx: &ActionCtx, input: &Value) -> Result<Value, ConnectorError> {
    let account = account_id(ctx, input)?;
    let mut body = Map::new();
    body.insert("name".into(), Value::String(req_str(input, "name")?));
    // VERIFY (spike M37a): `type` is required by the Admin API; "Office" is a valid default.
    body.insert(
        "type".into(),
        Value::String(opt_str(input, "type").unwrap_or_else(|| "Office".into())),
    );
    copy_strings(input, &mut body, &[("job_number", "jobNumber")]);
    for (src, dst) in [("start_date", "startDate"), ("end_date", "endDate")] {
        if let Some(d) = date_only(input, src) {
            body.insert(dst.into(), Value::String(d));
        }
    }
    if let Some(value) = opt_f64(input, "project_value") {
        let currency = opt_str(input, "currency").unwrap_or_else(|| "USD".into());
        body.insert(
            "projectValue".into(),
            json!({"value": value, "currency": currency}),
        );
    }
    let api = Forma::new(ctx)?;
    let url = api.url(&format!(
        "/construction/admin/v1/accounts/{}/projects",
        seg(&account)
    ));
    let project = api
        .json(HttpRequest::post(url).json(&Value::Object(body)))
        .await?;
    Ok(json!({
        "id": project.get("id").cloned().unwrap_or(Value::Null),
        "name": project.get("name").cloned().unwrap_or(Value::Null),
        "status": project.get("status").cloned().unwrap_or(Value::Null),
        "account_id": account,
        "project": project,
    }))
}

async fn add_project_user(ctx: &ActionCtx, input: &Value) -> Result<Value, ConnectorError> {
    let project = project_id(input)?;
    let email = req_str(input, "email")?;
    let mut products: Vec<Value> = list(input, "products")
        .into_iter()
        .map(|key| json!({"key": key, "access": "member"}))
        .collect();
    if products.is_empty() {
        products.push(json!({"key": "docs", "access": "member"}));
    }
    let api = Forma::new(ctx)?;
    // VERIFY (spike M37a): project users endpoint and product access shape.
    let url = api.url(&format!(
        "/construction/admin/v1/projects/{}/users",
        seg(&project)
    ));
    api.json(HttpRequest::post(url).json(&json!({"email": email, "products": products})))
        .await
}

async fn create_issue(ctx: &ActionCtx, input: &Value) -> Result<Value, ConnectorError> {
    let project = project_id(input)?;
    let status = opt_str(input, "status").unwrap_or_else(|| "open".into());
    let mut body = Map::new();
    body.insert("title".into(), Value::String(req_str(input, "title")?));
    body.insert(
        "issueSubtypeId".into(),
        Value::String(req_str(input, "issue_subtype_id")?),
    );
    // VERIFY (spike M37a): `published: true` makes non-draft issues visible to the project.
    body.insert("published".into(), Value::Bool(status != "draft"));
    body.insert("status".into(), Value::String(status));
    copy_strings(input, &mut body, &[("description", "description")]);
    if let Some(due) = date_only(input, "due_date") {
        body.insert("dueDate".into(), Value::String(due));
    }
    if let Some(user) = opt_str(input, "assigned_to") {
        body.insert("assignedTo".into(), Value::String(user));
        body.insert("assignedToType".into(), Value::String("user".into()));
    }
    let api = Forma::new(ctx)?;
    let url = api.url(&format!(
        "/construction/issues/v1/projects/{}/issues",
        seg(&project)
    ));
    api.json(HttpRequest::post(url).json(&Value::Object(body)))
        .await
}

async fn update_issue(ctx: &ActionCtx, input: &Value) -> Result<Value, ConnectorError> {
    let project = project_id(input)?;
    let issue = req_str(input, "issue_id")?;
    let mut body = Map::new();
    copy_strings(
        input,
        &mut body,
        &[
            ("status", "status"),
            ("title", "title"),
            ("description", "description"),
        ],
    );
    if let Some(due) = date_only(input, "due_date") {
        body.insert("dueDate".into(), Value::String(due));
    }
    if body.is_empty() {
        return Err(ConnectorError::invalid("Set at least one field to change"));
    }
    let api = Forma::new(ctx)?;
    let url = api.url(&format!(
        "/construction/issues/v1/projects/{}/issues/{}",
        seg(&project),
        seg(&issue)
    ));
    api.json(HttpRequest::new(Method::Patch, url).json(&Value::Object(body)))
        .await
}

/// Fetches one issue (also used by the event trigger).
pub(crate) async fn fetch_issue(
    ctx: &ActionCtx,
    project: &str,
    issue: &str,
) -> Result<Value, ConnectorError> {
    let api = Forma::new(ctx)?;
    let url = api.url(&format!(
        "/construction/issues/v1/projects/{}/issues/{}",
        seg(plain_id(project)),
        seg(issue)
    ));
    api.json(HttpRequest::get(url)).await
}

async fn get_issue(ctx: &ActionCtx, input: &Value) -> Result<Value, ConnectorError> {
    fetch_issue(ctx, &project_id(input)?, &req_str(input, "issue_id")?).await
}

async fn list_issues(ctx: &ActionCtx, input: &Value) -> Result<Value, ConnectorError> {
    let project = project_id(input)?;
    let limit = opt_f64(input, "limit")
        .unwrap_or(50.0)
        .clamp(1.0, 100.0)
        .to_string();
    let mut query = vec![("limit", limit.as_str())];
    let status = opt_str(input, "status");
    if let Some(s) = status.as_deref() {
        query.push(("filter[status]", s));
    }
    let api = Forma::new(ctx)?;
    let url = api.url(&format!(
        "/construction/issues/v1/projects/{}/issues",
        seg(&project)
    ));
    let body = api.json(HttpRequest::get(url).query(&query)).await?;
    let issues = results(&body);
    Ok(json!({"count": issues.len(), "issues": issues}))
}

async fn create_rfi(ctx: &ActionCtx, input: &Value) -> Result<Value, ConnectorError> {
    let project = project_id(input)?;
    let mut body = Map::new();
    body.insert("title".into(), Value::String(req_str(input, "title")?));
    body.insert(
        "question".into(),
        Value::String(req_str(input, "question")?),
    );
    copy_strings(input, &mut body, &[("assigned_to", "assignedTo")]);
    let api = Forma::new(ctx)?;
    // VERIFY (spike M37a): RFI v3 create path and body field names.
    let url = api.url(&format!(
        "/construction/rfis/v3/projects/{}/rfis",
        seg(&project)
    ));
    api.json(HttpRequest::post(url).json(&Value::Object(body)))
        .await
}

async fn get_project(ctx: &ActionCtx, input: &Value) -> Result<Value, ConnectorError> {
    let project = project_id(input)?;
    let api = Forma::new(ctx)?;
    let url = api.url(&format!(
        "/construction/admin/v1/projects/{}",
        seg(&project)
    ));
    api.json(HttpRequest::get(url)).await
}
