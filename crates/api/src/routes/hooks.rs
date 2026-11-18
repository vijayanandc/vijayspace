//! Public ingress: webhooks and app events.

use std::collections::BTreeMap;

use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::any,
};
use serde_json::json;
use weft_app::triggers::{InboundHttp, ingest_app_event, ingest_webhook};

use crate::{error::ApiError, state::ApiState};

pub fn router(state: &ApiState) -> Router<ApiState> {
    Router::new()
        .route("/hooks/{token}", any(webhook))
        .route("/hooks/app/{token}", any(app_event))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::middleware::limit_webhooks,
        ))
}

fn inbound(
    method: &Method,
    headers: &HeaderMap,
    query: BTreeMap<String, String>,
    body: Bytes,
) -> InboundHttp {
    InboundHttp {
        method: method.as_str().to_owned(),
        headers: headers
            .iter()
            .map(|(k, v)| {
                (
                    k.as_str().to_lowercase(),
                    String::from_utf8_lossy(v.as_bytes()).into_owned(),
                )
            })
            .collect(),
        query,
        body: body.to_vec(),
    }
}

async fn webhook(
    State(s): State<ApiState>,
    Path(token): Path<String>,
    method: Method,
    headers: HeaderMap,
    Query(q): Query<BTreeMap<String, String>>,
    body: Bytes,
) -> Response {
    match ingest_webhook(&s.deps, &token, &inbound(&method, &headers, q, body)).await {
        Ok(r) => (
            StatusCode::ACCEPTED,
            Json(json!({"status": r.status, "executions": r.executions})),
        )
            .into_response(),
        Err(e) => ApiError(e).into_response(),
    }
}

async fn app_event(
    State(s): State<ApiState>,
    Path(token): Path<String>,
    method: Method,
    headers: HeaderMap,
    Query(q): Query<BTreeMap<String, String>>,
    body: Bytes,
) -> Response {
    match ingest_app_event(&s.deps, &token, &inbound(&method, &headers, q, body)).await {
        Ok(r) => match r.response {
            Some(resp) => (StatusCode::OK, Json(resp)).into_response(),
            None => (
                StatusCode::ACCEPTED,
                Json(json!({"status": r.status, "executions": r.executions})),
            )
                .into_response(),
        },
        Err(e) => ApiError(e).into_response(),
    }
}
