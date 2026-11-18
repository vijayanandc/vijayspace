//! The connector error taxonomy (`docs/plan/04-workflow-engine.md` §9.1).

use serde_json::Value;
use weft_domain::error_codes;

use crate::http::HttpResponse;

/// Why a connector call failed. The class decides retry behaviour.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConnectorError {
    /// Network error, timeout, 5xx: retried automatically.
    #[error("{message}")]
    Transient { message: String },
    /// HTTP 429 / provider throttling: retried after `retry_after_secs`.
    #[error("rate limited: {message}")]
    RateLimited {
        retry_after_secs: Option<u64>,
        message: String,
    },
    /// Credentials rejected (after one refresh attempt where applicable).
    #[error("authorization expired or revoked: {message}")]
    AuthExpired { message: String },
    /// The app rejected the data (configuration or mapping error): not retried.
    #[error("{message}")]
    InvalidInput {
        field: Option<String>,
        message: String,
    },
    /// The record or resource does not exist.
    #[error("not found: {message}")]
    NotFound { message: String },
    /// Any other permanent failure.
    #[error("{message}")]
    Permanent {
        status: Option<u16>,
        message: String,
    },
    /// The connector does not implement this operation.
    #[error("not supported: {what}")]
    Unsupported { what: String },
    /// Held back by this platform's limits for the connection: the request was never sent.
    /// The runner waits and tries again without counting an attempt.
    #[error("held back by the connection's limits: {message}")]
    Throttled {
        retry_after_secs: u64,
        message: String,
    },
    /// A permanent failure with its own stable platform code (e.g. `script.error`).
    #[error("{message}")]
    Coded {
        code: &'static str,
        message: String,
        details: Value,
    },
}

/// The message of a timeout (see [`ConnectorError::timeout`]).
const TIMEOUT_MESSAGE: &str = "the app did not respond in time";

impl ConnectorError {
    /// The app did not answer in time. Retryable, but the request may have been applied: the runner does not
    /// repeat a non-idempotent action after a timeout.
    #[must_use]
    pub fn timeout() -> Self {
        Self::Transient {
            message: TIMEOUT_MESSAGE.to_owned(),
        }
    }

    /// Whether this is a timeout (made by [`ConnectorError::timeout`]).
    #[must_use]
    pub fn is_timeout(&self) -> bool {
        matches!(self, Self::Transient { message } if message == TIMEOUT_MESSAGE)
    }

    pub fn transient(message: impl Into<String>) -> Self {
        Self::Transient {
            message: message.into(),
        }
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::InvalidInput {
            field: None,
            message: message.into(),
        }
    }

    pub fn invalid_field(field: &str, message: impl Into<String>) -> Self {
        Self::InvalidInput {
            field: Some(field.to_owned()),
            message: message.into(),
        }
    }

    pub fn permanent(message: impl Into<String>) -> Self {
        Self::Permanent {
            status: None,
            message: message.into(),
        }
    }

    pub fn unsupported(what: impl Into<String>) -> Self {
        Self::Unsupported { what: what.into() }
    }

    /// Stable error code for history and the API.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Transient { .. } => error_codes::APP_UNAVAILABLE,
            Self::RateLimited { .. } => error_codes::APP_RATE_LIMITED,
            Self::AuthExpired { .. } => error_codes::CONNECTION_NEEDS_REAUTH,
            Self::InvalidInput { .. } => error_codes::APP_INVALID_INPUT,
            Self::NotFound { .. } => error_codes::APP_NOT_FOUND,
            Self::Permanent { .. } | Self::Unsupported { .. } => error_codes::APP_ERROR,
            Self::Coded { code, .. } => code,
            Self::Throttled { .. } => error_codes::CONNECTION_THROTTLED,
        }
    }

    /// Whether the engine should retry automatically.
    #[must_use]
    pub const fn retryable(&self) -> bool {
        matches!(
            self,
            Self::Transient { .. } | Self::RateLimited { .. } | Self::Throttled { .. }
        )
    }

    /// Provider-requested wait before retrying.
    #[must_use]
    pub const fn retry_after_secs(&self) -> Option<u64> {
        match self {
            Self::RateLimited {
                retry_after_secs, ..
            } => *retry_after_secs,
            Self::Throttled {
                retry_after_secs, ..
            } => Some(*retry_after_secs),
            Self::Transient { .. }
            | Self::AuthExpired { .. }
            | Self::InvalidInput { .. }
            | Self::NotFound { .. }
            | Self::Permanent { .. }
            | Self::Unsupported { .. }
            | Self::Coded { .. } => None,
        }
    }

    /// Extra structured details for history.
    #[must_use]
    pub fn details(&self) -> Value {
        match self {
            Self::InvalidInput { field: Some(f), .. } => serde_json::json!({"field": f}),
            Self::Coded { details, .. } => details.clone(),
            Self::Permanent {
                status: Some(s), ..
            } => serde_json::json!({"http_status": s}),
            Self::Transient { .. }
            | Self::RateLimited { .. }
            | Self::AuthExpired { .. }
            | Self::InvalidInput { field: None, .. }
            | Self::NotFound { .. }
            | Self::Permanent { status: None, .. }
            | Self::Unsupported { .. }
            | Self::Throttled { .. } => Value::Null,
        }
    }

    /// Maps a non-success HTTP response consistently. One place to fix mapping bugs.
    #[must_use]
    pub fn from_http(resp: &HttpResponse) -> Self {
        let body = resp.text();
        let message = format!(
            "HTTP {}: {}",
            resp.status,
            body.chars().take(500).collect::<String>()
        );
        match resp.status {
            401 => Self::AuthExpired { message },
            408 | 500..=599 => Self::Transient { message },
            429 => Self::RateLimited {
                retry_after_secs: resp
                    .header("retry-after")
                    .and_then(|v| v.trim().parse().ok()),
                message,
            },
            404 => Self::NotFound { message },
            400 | 409 | 412 | 413 | 415 | 422 => Self::InvalidInput {
                field: None,
                message,
            },
            status => Self::Permanent {
                status: Some(status),
                message,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resp(status: u16, headers: &[(&str, &str)]) -> HttpResponse {
        HttpResponse {
            status,
            headers: headers
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
            body: b"{\"error\":\"x\"}".to_vec(),
        }
    }

    #[test]
    fn http_statuses_map_to_classes() {
        assert!(matches!(
            ConnectorError::from_http(&resp(401, &[])),
            ConnectorError::AuthExpired { .. }
        ));
        assert!(matches!(
            ConnectorError::from_http(&resp(503, &[])),
            ConnectorError::Transient { .. }
        ));
        assert_eq!(
            ConnectorError::from_http(&resp(429, &[("Retry-After", "30")])).retry_after_secs(),
            Some(30)
        );
        assert!(matches!(
            ConnectorError::from_http(&resp(422, &[])),
            ConnectorError::InvalidInput { .. }
        ));
        assert!(matches!(
            ConnectorError::from_http(&resp(404, &[])),
            ConnectorError::NotFound { .. }
        ));
        assert!(matches!(
            ConnectorError::from_http(&resp(403, &[])),
            ConnectorError::Permanent {
                status: Some(403),
                ..
            }
        ));
        assert!(ConnectorError::from_http(&resp(500, &[])).retryable());
        assert!(!ConnectorError::from_http(&resp(400, &[])).retryable());
    }
}
