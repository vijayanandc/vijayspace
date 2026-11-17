//! Conversions between database values and domain types.

use serde::{Serialize, de::DeserializeOwned};
use weft_app::AppError;
use weft_domain::text::{enum_from_str, enum_to_str};

/// Maps a database error to an internal app error (the message is logged, not shown to clients).
pub(crate) fn db_err(e: sqlx::Error) -> AppError {
    AppError::internal(format!("database: {e}"))
}

/// True for duplicate-key errors (MySQL 1062).
pub(crate) fn is_duplicate(e: &sqlx::Error) -> bool {
    e.as_database_error()
        .is_some_and(sqlx::error::DatabaseError::is_unique_violation)
}

pub(crate) fn enum_str<T: Serialize>(v: &T) -> String {
    enum_to_str(v)
}

pub(crate) fn parse_enum<T: DeserializeOwned>(s: &str) -> Result<T, AppError> {
    enum_from_str(s).map_err(AppError::internal)
}

pub(crate) fn from_json<T: DeserializeOwned>(
    v: serde_json::Value,
    what: &str,
) -> Result<T, AppError> {
    serde_json::from_value(v).map_err(|e| AppError::internal(format!("corrupt {what}: {e}")))
}
