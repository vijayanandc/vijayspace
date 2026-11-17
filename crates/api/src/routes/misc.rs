//! Health, org and usage.

use axum::{Json, Router, extract::State, routing::get};
use serde_json::{Value, json};

use crate::{error::ApiResult, extract::Org, state::ApiState};

pub fn router() -> Router<ApiState> {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/readyz", get(ready))
        .route("/api/v1/org", get(org))
        .route("/api/v1/usage", get(usage))
}

async fn ready(State(s): State<ApiState>) -> ApiResult<&'static str> {
    weft_app::org::resolve_org(&s.deps, None).await?;
    Ok("ok")
}

async fn org(State(s): State<ApiState>, Org(org): Org) -> ApiResult<Json<Value>> {
    let o = s.deps.store.get_org(org).await?;
    Ok(Json(json!(o)))
}

async fn usage(State(s): State<ApiState>, Org(org): Org) -> ApiResult<Json<Value>> {
    let (period, tasks, executions) = weft_app::executions::usage(&s.deps, org).await?;
    Ok(Json(
        json!({"period": period, "tasks_used": tasks, "executions": executions}),
    ))
}
