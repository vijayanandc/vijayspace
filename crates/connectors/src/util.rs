//! Small input helpers shared by connectors.

use serde_json::{Map, Value};
use weft_connector_sdk::ConnectorError;

/// A required non-empty string field.
pub(crate) fn req_str(input: &Value, key: &str) -> Result<String, ConnectorError> {
    opt_str(input, key)
        .ok_or_else(|| ConnectorError::invalid_field(key, format!("`{key}` is required")))
}

/// An optional string field (numbers and booleans are converted; empty strings are `None`).
pub(crate) fn opt_str(input: &Value, key: &str) -> Option<String> {
    match input.get(key)? {
        Value::String(s) if !s.trim().is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::String(_) | Value::Null | Value::Array(_) | Value::Object(_) => None,
    }
}

/// An optional boolean (accepts `true`, `"true"`, `1`, `"yes"`).
pub(crate) fn opt_bool(input: &Value, key: &str) -> Option<bool> {
    match input.get(key)? {
        Value::Bool(b) => Some(*b),
        Value::String(s) => Some(matches!(
            s.trim().to_lowercase().as_str(),
            "true" | "1" | "yes" | "on"
        )),
        Value::Number(n) => Some(n.as_f64().is_some_and(|f| f != 0.0)),
        Value::Null | Value::Array(_) | Value::Object(_) => None,
    }
}

/// An optional number (strings are parsed).
pub(crate) fn opt_f64(input: &Value, key: &str) -> Option<f64> {
    match input.get(key)? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        Value::Null | Value::Bool(_) | Value::Array(_) | Value::Object(_) => None,
    }
}

/// Key/value input: accepts `{"k": "v"}` or `[{"key": "k", "value": "v"}]`.
pub(crate) fn key_values(input: &Value, key: &str) -> Vec<(String, String)> {
    let text = |v: &Value| match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        Value::Bool(_) | Value::Number(_) | Value::Array(_) | Value::Object(_) => v.to_string(),
    };
    match input.get(key) {
        Some(Value::Object(map)) => map.iter().map(|(k, v)| (k.clone(), text(v))).collect(),
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| {
                let k = item.get("key").and_then(Value::as_str)?;
                (!k.is_empty()).then(|| {
                    (
                        k.to_owned(),
                        item.get("value").map(text).unwrap_or_default(),
                    )
                })
            })
            .collect(),
        Some(Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_)) | None => {
            Vec::new()
        }
    }
}

/// An object field as a map (missing → empty).
#[allow(dead_code)] // used by app connectors
pub(crate) fn object(input: &Value, key: &str) -> Map<String, Value> {
    input
        .get(key)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

/// Comma/semicolon separated list or JSON array → trimmed non-empty strings.
pub(crate) fn list(input: &Value, key: &str) -> Vec<String> {
    match input.get(key) {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|v| {
                v.as_str()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
            })
            .collect(),
        Some(Value::String(s)) => s
            .split([',', ';'])
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect(),
        Some(Value::Null | Value::Bool(_) | Value::Number(_) | Value::Object(_)) | None => {
            Vec::new()
        }
    }
}
