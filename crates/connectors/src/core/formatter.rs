//! `core.formatter`: text, number, date and data utilities (pure, no connection).

use std::fmt::Write as _;

use chrono::{Duration, SecondsFormat};
use weft_connector_sdk::prelude::*;

use crate::util::{key_values, opt_f64, opt_str, req_str};

/// The formatter connector.
#[derive(Debug)]
pub struct FormatterConnector {
    manifest: ConnectorManifest,
}

impl FormatterConnector {
    #[must_use]
    pub fn new() -> Self {
        Self {
            manifest: manifest::manifest(),
        }
    }
}

impl Default for FormatterConnector {
    fn default() -> Self {
        Self::new()
    }
}

fn title_case(s: &str) -> String {
    s.split_whitespace()
        .map(|w| {
            let mut c = w.chars();
            c.next().map_or_else(String::new, |f| {
                f.to_uppercase()
                    .chain(c.flat_map(char::to_lowercase))
                    .collect()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn number(input: &Value, key: &str) -> Result<f64, ConnectorError> {
    opt_f64(input, key)
        .ok_or_else(|| ConnectorError::invalid_field(key, format!("`{key}` must be a number")))
}

fn json_number(n: f64) -> Value {
    if n.fract() == 0.0 && n.abs() < 9.0e15 {
        // Exact integer within the f64-safe range; the cast cannot truncate.
        #[allow(clippy::cast_possible_truncation)]
        let i = n as i64;
        json!(i)
    } else {
        json!(n)
    }
}

fn format_number(value: f64, decimals: usize, thousands: &str, decimal: &str) -> String {
    let fixed = format!("{:.*}", decimals, value.abs());
    let (int_part, frac_part) = fixed.split_once('.').unwrap_or((fixed.as_str(), ""));
    let mut grouped = String::new();
    for (i, c) in int_part.chars().enumerate() {
        if i > 0 && (int_part.len() - i) % 3 == 0 {
            grouped.push_str(thousands);
        }
        grouped.push(c);
    }
    let sign = if value < 0.0 { "-" } else { "" };
    if frac_part.is_empty() {
        format!("{sign}{grouped}")
    } else {
        format!("{sign}{grouped}{decimal}{frac_part}")
    }
}

fn extract(text: &str, pattern: &str, custom: Option<&str>) -> Result<Vec<String>, ConnectorError> {
    let re = match pattern {
        "email" => r"[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}",
        "phone" => r"\+?[0-9][0-9 ()\-]{6,}[0-9]",
        "number" => r"-?[0-9]+(?:\.[0-9]+)?",
        "url" => r"https?://[^\s<>]+",
        "custom" => custom
            .ok_or_else(|| ConnectorError::invalid_field("regex", "Enter a regular expression"))?,
        other => {
            return Err(ConnectorError::invalid_field(
                "pattern",
                format!("unknown pattern `{other}`"),
            ));
        }
    };
    let re = regex::RegexBuilder::new(re)
        .size_limit(1 << 16)
        .build()
        .map_err(|e| ConnectorError::invalid_field("regex", e.to_string()))?;
    Ok(re.find_iter(text).map(|m| m.as_str().to_owned()).collect())
}

fn parse_date(input: &Value) -> Result<chrono::DateTime<chrono::Utc>, ConnectorError> {
    let text = req_str(input, "date")?;
    weft_expr::parse_datetime(&text)
        .ok_or_else(|| ConnectorError::invalid_field("date", format!("`{text}` is not a date")))
}

#[allow(clippy::too_many_lines)] // one arm per action keeps the dispatch in one readable place
fn run(action: &str, input: &Value) -> Result<Value, ConnectorError> {
    match action {
        "text_transform" => {
            let text = req_str(input, "text")?;
            let result = match req_str(input, "operation")?.as_str() {
                "upper" => text.to_uppercase(),
                "lower" => text.to_lowercase(),
                "title" => title_case(&text),
                "capitalize" => {
                    let mut c = text.chars();
                    c.next()
                        .map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
                }
                "trim" => text.trim().to_owned(),
                "slugify" => {
                    weft_expr::render_value(&json!("{{ t | slugify }}"), &json!({"t": text}))
                        .map_err(|e| ConnectorError::invalid(e.to_string()))?
                        .as_str()
                        .unwrap_or_default()
                        .to_owned()
                }
                other => {
                    return Err(ConnectorError::invalid_field(
                        "operation",
                        format!("unknown operation `{other}`"),
                    ));
                }
            };
            Ok(json!({"result": result}))
        }
        "text_replace" => {
            let text = req_str(input, "text")?;
            let find = req_str(input, "find")?;
            let replace = opt_str(input, "replace").unwrap_or_default();
            let result = if crate::util::opt_bool(input, "regex").unwrap_or(false) {
                let re = regex::RegexBuilder::new(&find)
                    .size_limit(1 << 16)
                    .build()
                    .map_err(|e| ConnectorError::invalid_field("find", e.to_string()))?;
                re.replace_all(&text, replace.as_str()).into_owned()
            } else {
                text.replace(&find, &replace)
            };
            Ok(json!({"result": result}))
        }
        "text_split" => {
            let text = req_str(input, "text")?;
            let sep = opt_str(input, "separator").unwrap_or_else(|| ",".into());
            let parts: Vec<String> = text
                .split(sep.as_str())
                .map(|s| s.trim().to_owned())
                .collect();
            let item = opt_f64(input, "index").and_then(|i| {
                // Negative or fractional indexes select nothing.
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let idx = i as usize;
                (i >= 0.0 && i.fract() == 0.0)
                    .then(|| parts.get(idx).cloned())
                    .flatten()
            });
            Ok(json!({"result": parts, "item": item}))
        }
        "split_name" => {
            let full = req_str(input, "full_name")?;
            let mut words: Vec<&str> = full.split_whitespace().collect();
            let last = if words.len() > 1 {
                words.pop().unwrap_or_default()
            } else {
                ""
            };
            Ok(json!({"first_name": words.join(" "), "last_name": last}))
        }
        "extract" => {
            let all = extract(
                &req_str(input, "text")?,
                &req_str(input, "pattern")?,
                opt_str(input, "regex").as_deref(),
            )?;
            Ok(json!({"result": all.first(), "all": all}))
        }
        "number_format" => {
            let value = number(input, "value")?;
            let decimals = opt_f64(input, "decimals").unwrap_or(2.0).clamp(0.0, 10.0);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // clamped to 0..=10
            let decimals = decimals as usize;
            let thousands = opt_str(input, "thousands_separator").unwrap_or_else(|| ",".into());
            let decimal = opt_str(input, "decimal_separator").unwrap_or_else(|| ".".into());
            Ok(json!({"result": format_number(value, decimals, &thousands, &decimal)}))
        }
        "math" => {
            let a = number(input, "a")?;
            let b = number(input, "b")?;
            let result = match req_str(input, "operation")?.as_str() {
                "add" => a + b,
                "subtract" => a - b,
                "multiply" => a * b,
                "divide" if b == 0.0 => {
                    return Err(ConnectorError::invalid_field("b", "Cannot divide by zero"));
                }
                "divide" => a / b,
                "round" => {
                    let factor = 10f64.powf(b.clamp(0.0, 10.0).trunc());
                    (a * factor).round() / factor
                }
                other => {
                    return Err(ConnectorError::invalid_field(
                        "operation",
                        format!("unknown operation `{other}`"),
                    ));
                }
            };
            Ok(json!({"result": json_number(result)}))
        }
        "date_format" => {
            let date = parse_date(input)?;
            let tz: chrono_tz::Tz = opt_str(input, "timezone")
                .unwrap_or_else(|| "UTC".into())
                .parse()
                .map_err(|_| ConnectorError::invalid_field("timezone", "Unknown time zone"))?;
            let format = opt_str(input, "format").unwrap_or_else(|| "%Y-%m-%d".into());
            let mut out = String::new();
            write!(out, "{}", date.with_timezone(&tz).format(&format))
                .map_err(|_| ConnectorError::invalid_field("format", "Invalid date format"))?;
            Ok(json!({"result": out}))
        }
        "date_add" => {
            let date = parse_date(input)?;
            let amount = number(input, "amount")?;
            let seconds = match req_str(input, "unit")?.as_str() {
                "minutes" => amount * 60.0,
                "hours" => amount * 3_600.0,
                "days" => amount * 86_400.0,
                "weeks" => amount * 604_800.0,
                other => {
                    return Err(ConnectorError::invalid_field(
                        "unit",
                        format!("unknown unit `{other}`"),
                    ));
                }
            };
            #[allow(clippy::cast_possible_truncation)] // bounded by the range check below
            let delta = (seconds.abs() < 3.0e11)
                .then(|| Duration::try_seconds(seconds.round() as i64))
                .flatten()
                .ok_or_else(|| ConnectorError::invalid_field("amount", "Amount is out of range"))?;
            let result = date
                .checked_add_signed(delta)
                .ok_or_else(|| ConnectorError::invalid_field("amount", "Date out of range"))?;
            Ok(json!({"result": result.to_rfc3339_opts(SecondsFormat::Secs, true)}))
        }
        "lookup_table" => {
            let value = req_str(input, "value")?;
            let table = key_values(input, "table");
            let found = table
                .iter()
                .find(|(k, _)| *k == value)
                .map(|(_, v)| v.clone());
            let result = found
                .clone()
                .or_else(|| opt_str(input, "default"))
                .unwrap_or(value);
            Ok(json!({"result": result, "found": found.is_some()}))
        }
        "parse_json" => {
            let text = req_str(input, "text")?;
            let parsed: Value = serde_json::from_str(&text)
                .map_err(|e| ConnectorError::invalid_field("text", format!("Invalid JSON: {e}")))?;
            Ok(json!({"result": parsed}))
        }
        other => Err(ConnectorError::unsupported(other)),
    }
}

#[async_trait]
impl Connector for FormatterConnector {
    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    async fn execute(
        &self,
        _ctx: &ActionCtx,
        action: &str,
        input: Value,
    ) -> Result<Value, ConnectorError> {
        run(action, &input)
    }
}

mod manifest;
#[cfg(test)]
mod tests;
