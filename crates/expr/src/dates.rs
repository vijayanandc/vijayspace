//! Lenient date parsing shared by filters and condition operators.

use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeZone, Utc};

/// Parses RFC 3339, `YYYY-MM-DD HH:MM:SS`, `YYYY-MM-DD`, or epoch seconds/milliseconds.
pub(crate) fn parse_text(s: &str) -> Option<DateTime<Utc>> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }
    for fmt in ["%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%d %H:%M"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(s, fmt) {
            return Some(Utc.from_utc_datetime(&naive));
        }
    }
    if let Ok(date) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return date.and_hms_opt(0, 0, 0).map(|n| Utc.from_utc_datetime(&n));
    }
    s.parse::<i64>().ok().and_then(from_epoch)
}

/// Epoch seconds, or milliseconds when the number is too large to be seconds.
pub(crate) fn from_epoch(n: i64) -> Option<DateTime<Utc>> {
    if n.abs() > 100_000_000_000 {
        DateTime::from_timestamp_millis(n)
    } else {
        DateTime::from_timestamp(n, 0)
    }
}

/// Parses a JSON value (string or number) as a date-time.
pub(crate) fn parse_json(v: &serde_json::Value) -> Option<DateTime<Utc>> {
    match v {
        serde_json::Value::String(s) => parse_text(s),
        serde_json::Value::Number(n) => n.as_i64().and_then(from_epoch),
        serde_json::Value::Null
        | serde_json::Value::Bool(_)
        | serde_json::Value::Array(_)
        | serde_json::Value::Object(_) => None,
    }
}
