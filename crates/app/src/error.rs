//! `AppError`: what use cases return. The API maps `kind` to an HTTP status.

use weft_domain::{error_codes, flow::Issue};

/// Error categories (mapped to HTTP statuses by `weft-api`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    Validation,
    NotFound,
    Conflict,
    Unauthorized,
    Forbidden,
    BadGateway,
    Internal,
}

/// A use-case error with a stable code.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{code}: {message}")]
pub struct AppError {
    pub kind: ErrorKind,
    pub code: &'static str,
    pub message: String,
    pub issues: Vec<Issue>,
}

impl AppError {
    pub fn new(kind: ErrorKind, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind,
            code,
            message: message.into(),
            issues: Vec::new(),
        }
    }

    pub fn validation(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Validation, code, message)
    }

    pub fn not_found(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(ErrorKind::NotFound, code, message)
    }

    pub fn conflict(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Conflict, code, message)
    }

    /// No valid sign-in.
    pub fn unauthenticated(message: impl Into<String>) -> Self {
        Self::new(
            ErrorKind::Unauthorized,
            error_codes::AUTH_UNAUTHENTICATED,
            message,
        )
    }

    /// Signed in and a member, but the role does not allow this.
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Forbidden, error_codes::AUTH_FORBIDDEN, message)
    }

    /// An infrastructure failure (database, crypto). The message is logged, not shown to clients.
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Internal, error_codes::INTERNAL, message)
    }

    #[must_use]
    pub fn with_issues(mut self, issues: Vec<Issue>) -> Self {
        self.issues = issues;
        self
    }
}
