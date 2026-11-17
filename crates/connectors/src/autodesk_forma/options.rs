//! Dynamic dropdowns (accounts, projects, issue types) and the connection test.

use weft_connector_sdk::prelude::*;

use super::client::{Forma, account_id, plain_id, results, seg};
use crate::util::req_str;

/// Loads one option source.
pub(crate) async fn load(
    ctx: &ActionCtx,
    source: &str,
    input: &Value,
) -> Result<Vec<OptionItem>, ConnectorError> {
    match source {
        "accounts" => accounts(ctx).await,
        "projects" => projects(ctx, input).await,
        "issue_subtypes" => issue_subtypes(ctx, input).await,
        other => Err(ConnectorError::unsupported(format!("options `{other}`"))),
    }
}

fn text<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

/// Hubs from Data Management; the account ID is the hub ID without `b.`.
async fn accounts(ctx: &ActionCtx) -> Result<Vec<OptionItem>, ConnectorError> {
    let api = Forma::new(ctx)?;
    let body = api
        .json(HttpRequest::get(api.url("/project/v1/hubs")))
        .await?;
    Ok(results(&body)
        .iter()
        .filter_map(|hub| {
            let id = text(hub, "id")?;
            let name = hub
                .get("attributes")
                .and_then(|a| text(a, "name"))
                .unwrap_or(id);
            Some(OptionItem::new(plain_id(id), name))
        })
        .collect())
}

async fn projects(ctx: &ActionCtx, input: &Value) -> Result<Vec<OptionItem>, ConnectorError> {
    let account = account_id(ctx, input)?;
    let api = Forma::new(ctx)?;
    let url = api.url(&format!(
        "/construction/admin/v1/accounts/{}/projects",
        seg(&account)
    ));
    let body = api
        .json(HttpRequest::get(url).query(&[("limit", "200")]))
        .await?;
    Ok(results(&body)
        .iter()
        .filter_map(|p| {
            let id = text(p, "id")?;
            Some(OptionItem::new(plain_id(id), text(p, "name").unwrap_or(id)))
        })
        .collect())
}

/// Issue subtypes grouped under their types: label `Type › Subtype`.
async fn issue_subtypes(ctx: &ActionCtx, input: &Value) -> Result<Vec<OptionItem>, ConnectorError> {
    let project = req_str(input, "project_id")?;
    let api = Forma::new(ctx)?;
    let url = api.url(&format!(
        "/construction/issues/v1/projects/{}/issue-types",
        seg(plain_id(&project))
    ));
    let body = api
        .json(HttpRequest::get(url).query(&[("include", "subtypes")]))
        .await?;
    let mut out = Vec::new();
    for issue_type in results(&body) {
        let type_name = text(&issue_type, "title").unwrap_or("Issue");
        for sub in issue_type
            .get("subtypes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(id) = text(sub, "id") {
                out.push(OptionItem::new(
                    id,
                    format!("{type_name} › {}", text(sub, "title").unwrap_or(id)),
                ));
            }
        }
    }
    Ok(out)
}

/// Lists the user's hubs: proves the token works and shows which accounts are reachable.
pub(crate) async fn test_connection(ctx: &ActionCtx) -> Result<Value, ConnectorError> {
    let api = Forma::new(ctx)?;
    let body = api
        .json(HttpRequest::get(api.url("/project/v1/hubs")))
        .await?;
    let hubs = results(&body);
    let names: Vec<&str> = hubs
        .iter()
        .filter_map(|h| h.get("attributes").and_then(|a| text(a, "name")))
        .collect();
    Ok(json!({"hubs": hubs.len(), "accounts": names}))
}
