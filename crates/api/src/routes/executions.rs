//! Execution history and controls.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, post},
};
use base64::Engine as _;
use serde::Deserialize;
use serde_json::{Value, json};
use weft_app::{executions as uc, ports::ExecutionFilter};
use weft_domain::{
    ids::{ExecutionId, FlowId},
    model::ExecutionStatus,
};

use crate::{
    dto::{ExecutionDetailDto, ExecutionSummaryDto},
    error::{ApiResult, bad_id},
    extract::Acting,
    state::ApiState,
};

pub fn router() -> Router<ApiState> {
    Router::new()
        .route("/api/v1/executions", get(list))
        .route("/api/v1/executions/{id}", get(one))
        .route("/api/v1/executions/{id}/log", get(log))
        .route("/api/v1/executions/{id}/retry", post(retry))
        .route("/api/v1/executions/{id}/rerun", post(rerun))
        .route("/api/v1/executions/{id}/cancel", post(cancel))
}

fn id(raw: &str) -> Result<ExecutionId, crate::ApiError> {
    raw.parse().map_err(|_| bad_id("Execution"))
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    flow_id: Option<String>,
    status: Option<String>,
    limit: Option<u32>,
    cursor: Option<String>,
}

fn decode_cursor(c: &str) -> Option<(chrono::DateTime<chrono::Utc>, ExecutionId)> {
    let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(c)
        .ok()?;
    let text = String::from_utf8(raw).ok()?;
    let (at, id) = text.split_once('|')?;
    Some((
        chrono::DateTime::parse_from_rfc3339(at)
            .ok()?
            .with_timezone(&chrono::Utc),
        id.parse().ok()?,
    ))
}

fn encode_cursor(at: chrono::DateTime<chrono::Utc>, id: ExecutionId) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(format!("{}|{id}", at.to_rfc3339()))
}

async fn list(
    State(s): State<ApiState>,
    Acting(actor): Acting,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<Value>> {
    let filter = ExecutionFilter {
        flow_id: q
            .flow_id
            .as_deref()
            .filter(|f| !f.is_empty())
            .and_then(|f| f.parse::<FlowId>().ok()),
        status: q
            .status
            .as_deref()
            .filter(|f| !f.is_empty())
            .and_then(|f| weft_domain::text::enum_from_str::<ExecutionStatus>(f).ok()),
        limit: q.limit.unwrap_or(50),
        before: q.cursor.as_deref().and_then(decode_cursor),
    };
    let page = uc::list_executions(&s.deps, &actor, filter).await?;
    let items: Vec<ExecutionSummaryDto> = page
        .items
        .into_iter()
        .map(|(e, name)| ExecutionSummaryDto::new(&e, name))
        .collect();
    Ok(Json(
        json!({"items": items, "next_cursor": page.next.map(|(at, id)| encode_cursor(at, id))}),
    ))
}

async fn log(
    State(s): State<ApiState>,
    Acting(actor): Acting,
    Path(raw): Path<String>,
) -> ApiResult<Json<uc::ExecutionLog>> {
    Ok(Json(uc::execution_log(&s.deps, &actor, id(&raw)?).await?))
}

async fn one(
    State(s): State<ApiState>,
    Acting(actor): Acting,
    Path(raw): Path<String>,
) -> ApiResult<Json<ExecutionDetailDto>> {
    let (exe, steps) = uc::get_execution(&s.deps, &actor, id(&raw)?).await?;
    let flow_name = s
        .deps
        .store
        .get_flow(actor.org, exe.flow_id)
        .await?
        .map(|f| f.name)
        .unwrap_or_default();
    Ok(Json(ExecutionDetailDto {
        payloads_hidden: !actor.sees_payloads(),
        trigger_data: exe.trigger_data.clone(),
        summary: ExecutionSummaryDto::new(&exe, flow_name),
        steps,
    }))
}

async fn summary(
    s: &ApiState,
    actor: &weft_app::tenancy::Actor,
    id: ExecutionId,
) -> ApiResult<Json<ExecutionSummaryDto>> {
    let (exe, _) = uc::get_execution(&s.deps, actor, id).await?;
    let flow_name = s
        .deps
        .store
        .get_flow(actor.org, exe.flow_id)
        .await?
        .map(|f| f.name)
        .unwrap_or_default();
    Ok(Json(ExecutionSummaryDto::new(&exe, flow_name)))
}

async fn retry(
    State(s): State<ApiState>,
    Acting(actor): Acting,
    Path(raw): Path<String>,
) -> ApiResult<Json<ExecutionSummaryDto>> {
    let id = id(&raw)?;
    uc::retry_execution(&s.deps, &actor, id).await?;
    summary(&s, &actor, id).await
}

async fn rerun(
    State(s): State<ApiState>,
    Acting(actor): Acting,
    Path(raw): Path<String>,
) -> ApiResult<Json<Value>> {
    let new_id = uc::rerun_execution(&s.deps, &actor, id(&raw)?).await?;
    Ok(Json(json!({"execution_id": new_id})))
}

async fn cancel(
    State(s): State<ApiState>,
    Acting(actor): Acting,
    Path(raw): Path<String>,
) -> ApiResult<Json<ExecutionSummaryDto>> {
    let id = id(&raw)?;
    uc::cancel_execution(&s.deps, &actor, id).await?;
    summary(&s, &actor, id).await
}
