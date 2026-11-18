//! weft-api: the HTTP API (axum). Handlers are thin: parse → one use case → DTO.
//! The contract is `docs/api/README.md`.

mod dto;
mod error;
mod extract;
pub mod middleware;
mod routes;
mod state;

use std::time::Duration;

use axum::{
    Router,
    extract::DefaultBodyLimit,
    http::{HeaderName, HeaderValue, Method, Request, StatusCode},
};
use tower_http::{
    cors::CorsLayer,
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    timeout::{RequestBodyTimeoutLayer, TimeoutLayer},
    trace::{DefaultOnResponse, TraceLayer},
};
use tracing::Level;

/// The longest a request may take (a step test or a backfill page calls apps; app calls time out at 35 s).
const REQUEST_TIMEOUT: Duration = Duration::from_mins(2);
/// The longest a client may take to send its body (slow uploads hold memory and a task).
const BODY_TIMEOUT: Duration = Duration::from_secs(30);

pub use error::ApiError;
pub use state::ApiState;

/// Builds the full router.
pub fn router(state: ApiState, allowed_origins: &[String]) -> Router {
    let origins: Vec<HeaderValue> = allowed_origins
        .iter()
        .filter_map(|o| o.parse().ok())
        .collect();
    let cors = CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
        ])
        .allow_headers([
            axum::http::header::CONTENT_TYPE,
            axum::http::header::AUTHORIZATION,
            HeaderName::from_static("x-org-id"),
            axum::http::header::IF_MATCH,
        ])
        .expose_headers([HeaderName::from_static("x-request-id")])
        // Every call carries Authorization and X-Org-ID, so each would otherwise need its own preflight.
        .max_age(Duration::from_mins(10));
    let request_id = HeaderName::from_static("x-request-id");
    // Logged at INFO with a request ID; paths never include webhook tokens or query strings.
    let trace = TraceLayer::new_for_http()
        .make_span_with(|req: &Request<_>| {
            let id = req
                .headers()
                .get("x-request-id")
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default()
                .to_owned();
            tracing::info_span!("http", method = %req.method(), path = %middleware::log_path(req.uri()), request_id = %id)
        })
        .on_response(DefaultOnResponse::new().level(Level::INFO));
    Router::new()
        .merge(routes::router(&state))
        .layer(axum::middleware::from_fn(middleware::problem_errors))
        .layer(DefaultBodyLimit::max(6 * 1024 * 1024))
        .layer(RequestBodyTimeoutLayer::new(BODY_TIMEOUT))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::SERVICE_UNAVAILABLE,
            REQUEST_TIMEOUT,
        ))
        .layer(cors)
        .layer(trace)
        .layer(PropagateRequestIdLayer::new(request_id.clone()))
        .layer(SetRequestIdLayer::new(request_id, MakeRequestUuid))
        .with_state(state)
}
