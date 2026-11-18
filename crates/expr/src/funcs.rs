//! Pure helper functions shared by template filters and Rhai custom functions, so both behave the
//! same. No clock, no randomness, no IO. Errors are plain messages.

use std::fmt::Write as _;

use base64::Engine as _;
use chrono::{Duration, SecondsFormat};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

use crate::dates;

/// Largest compiled regex (bytes); patterns over this are rejected.
const REGEX_SIZE_LIMIT: usize = 1 << 16;

/// Formats a date (RFC 3339 text, epoch seconds or milliseconds) with a `strftime` format, in a
/// time zone (default UTC).
pub fn format_date(
    value: &serde_json::Value,
    format: Option<&str>,
    tz: Option<&str>,
) -> Result<String, String> {
    let parsed = dates::parse_json(value).ok_or_else(|| format!("not a date: {value}"))?;
    let format = format.unwrap_or("%Y-%m-%d");
    let tz: chrono_tz::Tz = tz
        .unwrap_or("UTC")
        .parse()
        .map_err(|_| "unknown time zone".to_owned())?;
    let mut out = String::new();
    // chrono panics on invalid specifiers with `to_string()`; `write!` reports an error instead.
    write!(out, "{}", parsed.with_timezone(&tz).format(format))
        .map_err(|_| format!("invalid date format `{format}`"))?;
    Ok(out)
}

/// Adds `amount` `unit`s (seconds, minutes, hours, days, weeks) to a date → RFC 3339 (UTC).
pub fn date_add(
    value: &serde_json::Value,
    amount: i64,
    unit: Option<&str>,
) -> Result<String, String> {
    let parsed = dates::parse_json(value).ok_or_else(|| format!("not a date: {value}"))?;
    let delta = match unit.unwrap_or("seconds") {
        "seconds" | "second" => Duration::try_seconds(amount),
        "minutes" | "minute" => Duration::try_minutes(amount),
        "hours" | "hour" => Duration::try_hours(amount),
        "days" | "day" => Duration::try_days(amount),
        "weeks" | "week" => Duration::try_weeks(amount),
        other => return Err(format!("unknown unit `{other}`")),
    }
    .ok_or_else(|| "duration out of range".to_owned())?;
    parsed
        .checked_add_signed(delta)
        .map(|d| d.to_rfc3339_opts(SecondsFormat::Secs, true))
        .ok_or_else(|| "date out of range".to_owned())
}

pub fn base64_encode(value: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(value.as_bytes())
}

pub fn base64_decode(value: &str) -> Result<String, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(value.trim().as_bytes())
        .map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Lower-case hex SHA-256.
pub fn sha256_hex(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

/// Lower-case hex HMAC-SHA256 of `value` with `key`.
pub fn hmac_sha256_hex(value: &str, key: &str) -> Result<String, String> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key.as_bytes()).map_err(|e| e.to_string())?;
    mac.update(value.as_bytes());
    Ok(hex::encode(mac.finalize().into_bytes()))
}

fn regex(pattern: &str) -> Result<regex::Regex, String> {
    regex::RegexBuilder::new(pattern)
        .size_limit(REGEX_SIZE_LIMIT)
        .build()
        .map_err(|e| e.to_string())
}

/// Replaces every match of `pattern` (`$1` refers to groups).
pub fn regex_replace(value: &str, pattern: &str, replacement: &str) -> Result<String, String> {
    Ok(regex(pattern)?.replace_all(value, replacement).into_owned())
}

/// Whether `pattern` matches anywhere in `value`.
pub fn regex_is_match(value: &str, pattern: &str) -> Result<bool, String> {
    Ok(regex(pattern)?.is_match(value))
}

pub fn url_encode(value: &str) -> String {
    urlencoding::encode(value).into_owned()
}

pub fn url_decode(value: &str) -> Result<String, String> {
    urlencoding::decode(value)
        .map(std::borrow::Cow::into_owned)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn helpers_behave_like_the_template_filters() {
        assert_eq!(
            format_date(&json!("2026-01-02T03:04:05Z"), Some("%d %b %Y"), None).unwrap(),
            "02 Jan 2026"
        );
        assert_eq!(
            format_date(&json!("2026-01-01T20:00:00Z"), None, Some("Asia/Kolkata")).unwrap(),
            "2026-01-02"
        );
        assert_eq!(
            date_add(&json!("2026-01-31T00:00:00Z"), 1, Some("days")).unwrap(),
            "2026-02-01T00:00:00Z"
        );
        assert!(date_add(&json!("x"), 1, None).is_err());
        assert!(format_date(&json!("2026-01-01"), Some("%Q"), None).is_err());
        assert_eq!(base64_decode(&base64_encode("héllo")).unwrap(), "héllo");
        assert_eq!(sha256_hex("abc").len(), 64);
        assert_eq!(hmac_sha256_hex("msg", "key").unwrap().len(), 64);
        assert_eq!(regex_replace("a1b22", r"\d+", "#").unwrap(), "a#b#");
        assert!(regex_is_match("Order-42", r"^Order-\d+$").unwrap());
        assert!(regex_replace("x", "(", "").is_err());
        assert_eq!(url_decode(&url_encode("a b&c")).unwrap(), "a b&c");
    }
}
