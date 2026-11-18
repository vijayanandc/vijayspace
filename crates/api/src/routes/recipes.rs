//! Prebuilt integrations (recipes).

use std::collections::BTreeMap;

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use weft_app::recipes;

use crate::{
    error::ApiResult,
    extract::{Acting, SignedIn},
    state::ApiState,
};

pub fn router() -> Router<ApiState> {
    Router::new()
        .route("/api/v1/recipes", get(list))
        .route("/api/v1/recipes/{key}/install", post(install))
}

async fn list(_: SignedIn) -> Json<Value> {
    Json(json!(recipes::catalog()))
}

#[derive(Debug, Deserialize)]
struct InstallBody {
    #[serde(default)]
    connections: BTreeMap<String, String>,
}

async fn install(
    State(s): State<ApiState>,
    Acting(actor): Acting,
    Path(key): Path<String>,
    Json(b): Json<InstallBody>,
) -> ApiResult<Json<Value>> {
    let ids = recipes::install(&s.deps, &actor, &key, &b.connections).await?;
    Ok(Json(json!({"flow_ids": ids})))
}
