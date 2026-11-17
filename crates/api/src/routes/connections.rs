//! Connections and OAuth.

use std::collections::BTreeMap;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use weft_app::connections as uc;
use weft_domain::{ids::ConnectionId, model::Connection};

use crate::{
    error::{ApiResult, bad_id},
    extract::Org,
    state::ApiState,
};

pub fn router() -> Router<ApiState> {
    Router::new()
        .route("/api/v1/connections", get(list).post(create))
        .route("/api/v1/connections/oauth/start", post(oauth_start))
        .route(
            "/api/v1/connections/{id}",
            get(one).patch(update).delete(remove),
        )
        .route("/api/v1/connections/{id}/test", post(test))
        .route("/oauth/callback", get(callback))
}

fn id(raw: &str) -> Result<ConnectionId, crate::ApiError> {
    raw.parse().map_err(|_| bad_id("Connection"))
}

#[derive(Debug, Deserialize)]
struct CreateBody {
    connector: String,
    name: String,
    #[serde(default)]
    fields: Map<String, Value>,
}

async fn list(State(s): State<ApiState>, Org(org): Org) -> ApiResult<Json<Vec<Connection>>> {
    Ok(Json(uc::list_connections(&s.deps, org).await?))
}

async fn create(
    State(s): State<ApiState>,
    Org(org): Org,
    Json(b): Json<CreateBody>,
) -> ApiResult<(StatusCode, Json<Connection>)> {
    Ok((
        StatusCode::CREATED,
        Json(uc::create_connection(&s.deps, org, &b.connector, &b.name, &b.fields).await?),
    ))
}

async fn one(
    State(s): State<ApiState>,
    Org(org): Org,
    Path(raw): Path<String>,
) -> ApiResult<Json<Connection>> {
    Ok(Json(uc::get_connection(&s.deps, org, id(&raw)?).await?))
}

#[derive(Debug, Deserialize)]
struct UpdateBody {
    name: Option<String>,
    fields: Option<Map<String, Value>>,
}

async fn update(
    State(s): State<ApiState>,
    Org(org): Org,
    Path(raw): Path<String>,
    Json(b): Json<UpdateBody>,
) -> ApiResult<Json<Connection>> {
    Ok(Json(
        uc::update_connection(
            &s.deps,
            org,
            id(&raw)?,
            b.name.as_deref(),
            b.fields.as_ref(),
        )
        .await?,
    ))
}

async fn remove(
    State(s): State<ApiState>,
    Org(org): Org,
    Path(raw): Path<String>,
) -> ApiResult<StatusCode> {
    uc::delete_connection(&s.deps, org, id(&raw)?).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn test(
    State(s): State<ApiState>,
    Org(org): Org,
    Path(raw): Path<String>,
) -> ApiResult<Json<uc::ConnectionTest>> {
    Ok(Json(uc::test_connection(&s.deps, org, id(&raw)?).await?))
}

#[derive(Debug, Deserialize)]
struct OAuthStartBody {
    connector: String,
    name: String,
    connection_id: Option<String>,
    #[serde(default)]
    fields: Map<String, Value>,
    return_to: String,
}

async fn oauth_start(
    State(s): State<ApiState>,
    Org(org): Org,
    Json(b): Json<OAuthStartBody>,
) -> ApiResult<Json<Value>> {
    let existing = b.connection_id.as_deref().map(id).transpose()?;
    let url = uc::oauth_start(
        &s.deps,
        org,
        &b.connector,
        &b.name,
        existing,
        &b.fields,
        &b.return_to,
    )
    .await?;
    Ok(Json(json!({"authorize_url": url})))
}

async fn callback(
    State(s): State<ApiState>,
    Query(q): Query<BTreeMap<String, String>>,
) -> Response {
    match uc::oauth_callback(&s.deps, &q).await {
        Ok(url) => Redirect::to(&url).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            format!("Could not complete the connection: {}", e.message),
        )
            .into_response(),
    }
}
