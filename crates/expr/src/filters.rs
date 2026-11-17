//! Custom template filters (in addition to minijinja's built-ins such as `upper`, `default`, `join`).
//!
//! Every filter is pure and deterministic: no clock, no randomness, no IO.

use std::fmt::Write as _;

use base64::Engine as _;
use chrono::{Duration, SecondsFormat};
use hmac::{Hmac, Mac};
use minijinja::{Environment, Error, ErrorKind, Value};
use sha2::{Digest, Sha256};

use crate::dates;

pub(crate) fn register(env: &mut Environment<'static>) {
    env.add_filter("translate", translate);
    env.add_filter("date", date);
    env.add_filter("date_add", date_add);
    env.add_filter("slugify", slugify);
    env.add_filter("from_json", from_json);
    env.add_filter("to_json", to_json);
    env.add_filter("base64_encode", base64_encode);
    env.add_filter("base64_decode", base64_decode);
    env.add_filter("sha256", sha256);
    env.add_filter("hmac_sha256", hmac_sha256);
    env.add_filter("regex_replace", regex_replace);
    env.add_filter("truncate", truncate);
    env.add_filter("pluck", pluck);
    env.add_filter("get", get_path);
    env.add_filter("digits", digits);
}

fn invalid(msg: impl Into<String>) -> Error {
    Error::new(ErrorKind::InvalidOperation, msg.into())
}

fn text_of(value: &Value) -> String {
    value
        .as_str()
        .map_or_else(|| value.to_string(), str::to_owned)
}

/// Value map: `{{ stage | translate({"Closed Won": "Open"}, "New") }}`.
/// Unmapped values return `default` when given, otherwise the original value.
fn translate(value: Value, mapping: Value, default: Option<Value>) -> Value {
    let key = Value::from(text_of(&value));
    match mapping.get_item(&key) {
        Ok(found) if !found.is_undefined() => found,
        Ok(_) | Err(_) => default.unwrap_or(value),
    }
}

fn to_serde(value: &Value) -> Result<serde_json::Value, Error> {
    serde_json::to_value(value).map_err(|e| invalid(e.to_string()))
}

/// Formats a date: `{{ created | date("%d %b %Y", "Asia/Kolkata") }}`.
fn date(value: Value, format: Option<String>, tz: Option<String>) -> Result<String, Error> {
    let parsed = dates::parse_json(&to_serde(&value)?)
        .ok_or_else(|| invalid(format!("not a date: {value}")))?;
    let format = format.unwrap_or_else(|| "%Y-%m-%d".to_owned());
    let tz: chrono_tz::Tz = tz
        .as_deref()
        .unwrap_or("UTC")
        .parse()
        .map_err(|_| invalid("unknown time zone"))?;
    let mut out = String::new();
    // chrono panics on invalid format specifiers when using `to_string()`; `write!` reports an error instead.
    write!(out, "{}", parsed.with_timezone(&tz).format(&format))
        .map_err(|_| invalid(format!("invalid date format `{format}`")))?;
    Ok(out)
}

/// Adds a duration: `{{ due | date_add(2, "days") }}` → RFC 3339 (UTC).
fn date_add(value: Value, amount: i64, unit: Option<String>) -> Result<String, Error> {
    let parsed = dates::parse_json(&to_serde(&value)?)
        .ok_or_else(|| invalid(format!("not a date: {value}")))?;
    let delta = match unit.as_deref().unwrap_or("seconds") {
        "seconds" | "second" => Duration::try_seconds(amount),
        "minutes" | "minute" => Duration::try_minutes(amount),
        "hours" | "hour" => Duration::try_hours(amount),
        "days" | "day" => Duration::try_days(amount),
        "weeks" | "week" => Duration::try_weeks(amount),
        other => return Err(invalid(format!("unknown unit `{other}`"))),
    }
    .ok_or_else(|| invalid("duration out of range"))?;
    parsed
        .checked_add_signed(delta)
        .map(|d| d.to_rfc3339_opts(SecondsFormat::Secs, true))
        .ok_or_else(|| invalid("date out of range"))
}

fn slugify(value: String) -> String {
    let mut out = String::with_capacity(value.len());
    let mut dash = false;
    for c in value.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    out.trim_end_matches('-').to_owned()
}

fn from_json(value: String) -> Result<Value, Error> {
    let parsed: serde_json::Value =
        serde_json::from_str(&value).map_err(|e| invalid(e.to_string()))?;
    Ok(Value::from_serialize(&parsed))
}

fn to_json(value: Value) -> Result<String, Error> {
    serde_json::to_string(&value).map_err(|e| invalid(e.to_string()))
}

fn base64_encode(value: String) -> String {
    base64::engine::general_purpose::STANDARD.encode(value.as_bytes())
}

fn base64_decode(value: String) -> Result<String, Error> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(value.trim().as_bytes())
        .map_err(|e| invalid(e.to_string()))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn sha256(value: String) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

fn hmac_sha256(value: String, key: String) -> Result<String, Error> {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(key.as_bytes()).map_err(|e| invalid(e.to_string()))?;
    mac.update(value.as_bytes());
    Ok(hex::encode(mac.finalize().into_bytes()))
}

fn regex_replace(value: String, pattern: String, replacement: String) -> Result<String, Error> {
    let re = regex::RegexBuilder::new(&pattern)
        .size_limit(1 << 16)
        .build()
        .map_err(|e| invalid(e.to_string()))?;
    Ok(re.replace_all(&value, replacement.as_str()).into_owned())
}

fn truncate(value: String, length: usize) -> String {
    value.chars().take(length).collect()
}

fn pluck(list: Value, key: String) -> Result<Value, Error> {
    let mut out = Vec::new();
    for item in list.try_iter()? {
        out.push(item.get_attr(&key).unwrap_or(Value::UNDEFINED));
    }
    Ok(Value::from(out))
}

fn get_path(value: Value, path: String) -> Value {
    let mut current = value;
    for part in path.split('.').filter(|p| !p.is_empty()) {
        let key = part
            .parse::<i64>()
            .map_or_else(|_| Value::from(part), Value::from);
        current = current.get_item(&key).unwrap_or(Value::UNDEFINED);
        if current.is_undefined() {
            break;
        }
    }
    current
}

fn digits(value: String) -> String {
    value.chars().filter(char::is_ascii_digit).collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::render_value;

    fn r(tpl: &str, scope: &serde_json::Value) -> serde_json::Value {
        render_value(&json!(tpl), scope).unwrap()
    }

    #[test]
    fn translate_maps_and_defaults() {
        let s = json!({"v": "Closed Won", "w": "Other"});
        assert_eq!(
            r("{{ v | translate({'Closed Won': 'Open'}, 'New') }}", &s),
            json!("Open")
        );
        assert_eq!(
            r("{{ w | translate({'Closed Won': 'Open'}, 'New') }}", &s),
            json!("New")
        );
        assert_eq!(
            r("{{ w | translate({'Closed Won': 'Open'}) }}", &s),
            json!("Other")
        );
    }

    #[test]
    fn date_formats_in_time_zone() {
        let s = json!({"d": "2026-03-08T07:30:00Z"});
        assert_eq!(
            r("{{ d | date('%Y-%m-%d %H:%M', 'Asia/Kolkata') }}", &s),
            json!("2026-03-08 13:00")
        );
        assert_eq!(r("{{ d | date }}", &s), json!("2026-03-08"));
    }

    #[test]
    fn invalid_date_format_is_an_error_not_a_panic() {
        let s = json!({"d": "2026-03-08T07:30:00Z"});
        assert!(render_value(&json!("{{ d | date('%Q%') }}"), &s).is_err());
    }

    #[test]
    fn date_add_supports_units() {
        let s = json!({"d": "2026-01-31T00:00:00Z"});
        assert_eq!(
            r("{{ d | date_add(1, 'days') }}", &s),
            json!("2026-02-01T00:00:00Z")
        );
        assert!(render_value(&json!("{{ d | date_add(1, 'fortnights') }}"), &s).is_err());
    }

    #[test]
    fn misc_filters() {
        let s = json!({"t": "Hello, World!", "p": "+91 98450-12345", "rows": [{"id": 1}, {"id": 2}], "o": {"a": {"b": [10, 20]}}});
        assert_eq!(r("{{ t | slugify }}", &s), json!("hello-world"));
        assert_eq!(r("{{ p | digits }}", &s), json!("919845012345"));
        assert_eq!(r("{{ rows | pluck('id') }}", &s), json!([1, 2]));
        assert_eq!(r("{{ o | get('a.b.1') }}", &s), json!(20));
        assert_eq!(r("{{ t | truncate(5) }}", &s), json!("Hello"));
        assert_eq!(r("{{ t | sha256 | truncate(8) }}", &s), json!("dffd6021"));
        assert_eq!(
            r("{{ t | base64_encode | base64_decode }}", &s),
            json!("Hello, World!")
        );
        assert_eq!(r("{{ '{\"a\": 1}' | from_json }}", &s), json!({"a": 1}));
        assert_eq!(
            r("{{ t | regex_replace('[aeiou]', '') }}", &s),
            json!("Hll, Wrld!")
        );
    }
}
