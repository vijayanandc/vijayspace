//! `AppError` → RFC 9457 problem+json.

use axum::{
    Json,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use serde_json::json;
use weft_app::{AppError, ErrorKind};

/// An API error response.
#[derive(Debug)]
pub struct ApiError(pub AppError);

impl From<AppError> for ApiError {
    fn from(e: AppError) -> Self {
        Self(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let e = self.0;
        let status = match e.kind {
            ErrorKind::Validation => StatusCode::UNPROCESSABLE_ENTITY,
            ErrorKind::NotFound => StatusCode::NOT_FOUND,
            ErrorKind::Conflict => StatusCode::CONFLICT,
            ErrorKind::Unauthorized => StatusCode::UNAUTHORIZED,
            ErrorKind::BadGateway => StatusCode::BAD_GATEWAY,
            ErrorKind::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let detail = if e.kind == ErrorKind::Internal {
            tracing::error!(error = %e.message, "internal error");
            "Something went wrong on our side. Try again.".to_owned()
        } else {
            e.message.clone()
        };
        let mut body = json!({
            "type": "about:blank",
            "title": status.canonical_reason().unwrap_or("Error"),
            "status": status.as_u16(),
            "code": e.code,
            "detail": detail,
        });
        if !e.issues.is_empty() {
            body["issues"] = json!(e.issues);
        }
        (
            status,
            [(header::CONTENT_TYPE, "application/problem+json")],
            Json(body),
        )
            .into_response()
    }
}

/// Result type for handlers.
pub type ApiResult<T> = Result<T, ApiError>;

/// A 422 for malformed path IDs.
pub fn bad_id(what: &str) -> ApiError {
    ApiError(AppError::not_found(
        weft_domain::error_codes::NOT_FOUND,
        format!("{what} not found"),
    ))
}
