//! Connector catalog and dynamic options/fields.

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use serde_json::{Value, json};
use weft_app::AppError;
use weft_connector_sdk::ConnectorManifest;
use weft_domain::error_codes;

use crate::{dto::DynamicRequest, error::ApiResult, extract::Org, state::ApiState};

pub fn router() -> Router<ApiState> {
    Router::new()
        .route("/api/v1/connectors", get(list))
        .route("/api/v1/connectors/{key}", get(one))
        .route("/api/v1/connectors/{key}/options/{source}", post(options))
        .route("/api/v1/connectors/{key}/fields/{source}", post(fields))
}

async fn list(State(s): State<ApiState>) -> Json<Vec<ConnectorManifest>> {
    Json(s.deps.connectors.manifests().cloned().collect())
}

async fn one(
    State(s): State<ApiState>,
    Path(key): Path<String>,
) -> ApiResult<Json<ConnectorManifest>> {
    let c = s
        .deps
        .connectors
        .get(&key)
        .ok_or_else(|| AppError::not_found(error_codes::CONNECTOR_NOT_FOUND, "Unknown app"))?;
    Ok(Json(c.manifest().clone()))
}

async fn options(
    State(s): State<ApiState>,
    Org(org): Org,
    Path((key, source)): Path<(String, String)>,
    Json(req): Json<DynamicRequest>,
) -> ApiResult<Json<Value>> {
    let options = weft_app::connections::load_options(
        &s.deps,
        org,
        &key,
        &source,
        req.connection_id.as_deref(),
        &req.input,
    )
    .await?;
    Ok(Json(json!({"options": options})))
}

async fn fields(
    State(s): State<ApiState>,
    Org(org): Org,
    Path((key, source)): Path<(String, String)>,
    Json(req): Json<DynamicRequest>,
) -> ApiResult<Json<Value>> {
    let fields = weft_app::connections::load_fields(
        &s.deps,
        org,
        &key,
        &source,
        req.connection_id.as_deref(),
        &req.input,
    )
    .await?;
    Ok(Json(json!({"fields": fields})))
}
