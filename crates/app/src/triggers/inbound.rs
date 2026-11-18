//! Reading inbound webhook requests: the body by content type, the headers a flow may see, and constant-time
//! comparison of secrets.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

const HIDDEN_HEADERS: [&str; 3] = ["authorization", "cookie", "proxy-authorization"];

fn parse_form(body: &str) -> Value {
    let mut map = Map::new();
    for pair in body.split('&').filter(|p| !p.is_empty()) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        let decode = |s: &str| {
            urlencoding::decode(&s.replace('+', " "))
                .map_or_else(|_| s.to_owned(), std::borrow::Cow::into_owned)
        };
        map.insert(decode(k), Value::String(decode(v)));
    }
    Value::Object(map)
}

/// Parses a body by content type: JSON, form-urlencoded, or text.
#[must_use]
pub fn parse_body(content_type: &str, body: &[u8]) -> Value {
    let text = String::from_utf8_lossy(body);
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Value::Null;
    }
    if (content_type.contains("json") || trimmed.starts_with('{') || trimmed.starts_with('['))
        && let Ok(v) = serde_json::from_str(trimmed)
    {
        return v;
    }
    if content_type.contains("x-www-form-urlencoded") {
        return parse_form(trimmed);
    }
    Value::String(text.into_owned())
}

pub(super) fn visible_headers(headers: &BTreeMap<String, String>) -> Map<String, Value> {
    headers
        .iter()
        .filter(|(k, _)| !HIDDEN_HEADERS.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), Value::String(v.clone())))
        .collect()
}

/// Constant-time string comparison for secrets.
#[must_use]
pub fn constant_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn bodies_are_parsed_by_content_type() {
        assert_eq!(
            parse_body("application/json", br#"{"a":1}"#),
            json!({"a": 1})
        );
        assert_eq!(
            parse_body(
                "application/x-www-form-urlencoded",
                b"name=Ann+Lee&e=a%40b.io"
            ),
            json!({"name": "Ann Lee", "e": "a@b.io"})
        );
        assert_eq!(parse_body("text/plain", b"hello"), json!("hello"));
        assert_eq!(parse_body("", b""), Value::Null);
        assert_eq!(parse_body("", b"[1,2]"), json!([1, 2]));
    }

    #[test]
    fn constant_time_compare() {
        assert!(constant_eq("abc", "abc"));
        assert!(!constant_eq("abc", "abd"));
        assert!(!constant_eq("abc", "ab"));
    }
}
