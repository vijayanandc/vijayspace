//! Conversions between serde-represented enums and their stored string form.

use serde::{Serialize, de::DeserializeOwned};

/// The `snake_case` string a unit enum variant serializes to (e.g. `ExecutionStatus::Running` → `"running"`).
#[must_use]
pub fn enum_to_str<T: Serialize>(value: &T) -> String {
    match serde_json::to_value(value) {
        Ok(serde_json::Value::String(s)) => s,
        Ok(other) => other.to_string(),
        Err(_) => String::new(),
    }
}

/// Parses a stored string back into a unit enum variant.
pub fn enum_from_str<T: DeserializeOwned>(s: &str) -> Result<T, String> {
    serde_json::from_value(serde_json::Value::String(s.to_owned()))
        .map_err(|e| format!("unknown value `{s}`: {e}"))
}
