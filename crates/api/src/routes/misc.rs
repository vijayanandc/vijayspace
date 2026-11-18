//! Health and usage.

use axum::{Json, Router, extract::State, routing::get};
use serde_json::{Value, json};

use crate::{error::ApiResult, extract::Acting, state::ApiState};

pub fn router() -> Router<ApiState> {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/readyz", get(ready))
        .route("/api/v1/usage", get(usage))
}

async fn ready(State(s): State<ApiState>) -> ApiResult<&'static str> {
    weft_app::org::resolve_org(&s.deps, None).await?;
    Ok("ok")
}

async fn usage(State(s): State<ApiState>, Acting(actor): Acting) -> ApiResult<Json<Value>> {
    let (period, tasks, executions) = weft_app::executions::usage(&s.deps, &actor).await?;
    Ok(Json(
        json!({"period": period, "tasks_used": tasks, "executions": executions}),
    ))
}
