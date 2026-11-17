//! weft-api: the HTTP API (axum). Handlers are thin: parse → one use case → DTO.
//! The contract is `docs/api/README.md`.

mod dto;
mod error;
mod extract;
mod routes;
mod state;

use axum::{
    Router,
    extract::DefaultBodyLimit,
    http::{HeaderName, HeaderValue, Method},
};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

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
            HeaderName::from_static("x-org-id"),
            axum::http::header::IF_MATCH,
        ]);
    Router::new()
        .merge(routes::router())
        .layer(DefaultBodyLimit::max(6 * 1024 * 1024))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
