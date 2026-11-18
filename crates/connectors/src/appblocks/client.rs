//! HTTP access to an Appblocks app (FSM, HMS, …): auth headers, `?trigger=`, error mapping.
//!
//! Shapes follow the Appblocks `openapi.yaml`: records are flat JSON objects keyed by field
//! `api_name`, errors are `{"error": "…", "code": "…"}`, and token requests are rate limited
//! per org per UTC day (`docs/api_token.md`).

use chrono::{DateTime, Duration, Utc};
use weft_connector_sdk::prelude::*;

/// Authenticated access to one Appblocks org.
#[derive(Debug)]
pub(super) struct Client {
    base: String,
    org_id: String,
    token: String,
}

/// Reads a config value that may have been stored as a string or a number.
fn config_value(ctx: &ActionCtx, key: &str) -> Result<String, ConnectorError> {
    let config = ctx.conn()?.config();
    match config.get(key) {
        Some(Value::String(s)) if !s.trim().is_empty() => Ok(s.trim().to_owned()),
        Some(Value::Number(n)) => Ok(n.to_string()),
        Some(
            Value::String(_) | Value::Null | Value::Bool(_) | Value::Array(_) | Value::Object(_),
        )
        | None => Err(ConnectorError::invalid(format!(
            "The connection is missing `{key}`; edit the connection"
        ))),
    }
}

/// Module API names are identifiers; rejecting anything else prevents path injection from templates.
pub(super) fn check_module(module: &str) -> Result<&str, ConnectorError> {
    let mut chars = module.chars();
    let ok = chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    if ok {
        Ok(module)
    } else {
        Err(ConnectorError::invalid_field(
            "module",
            format!("`{module}` is not a valid module name"),
        ))
    }
}

/// Record `api_id`s are ULID-like tokens.
pub(super) fn check_id(field: &str, id: &str) -> Result<String, ConnectorError> {
    let id = id.trim();
    let ok = !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if ok {
        Ok(id.to_owned())
    } else {
        Err(ConnectorError::invalid_field(
            field,
            format!("`{id}` is not a valid record ID"),
        ))
    }
}

impl Client {
    pub(super) async fn new(ctx: &ActionCtx) -> Result<Self, ConnectorError> {
        let base = config_value(ctx, "base_url")?
            .trim_end_matches('/')
            .to_owned();
        let org_id = config_value(ctx, "org_id")?;
        let token = ctx.secret_str("api_token").await?;
        Ok(Self {
            base,
            org_id,
            token,
        })
    }

    /// A request to `{base_url}{path}` with the token and org headers.
    pub(super) fn request(&self, method: Method, path: &str) -> HttpRequest {
        HttpRequest::new(method, format!("{}{path}", self.base))
            .bearer(&self.token)
            .header("X-Org-ID", self.org_id.clone())
            .header("Accept", "application/json")
    }

    /// Sends and returns the JSON body (`null` for 204), mapping errors.
    pub(super) async fn send(
        &self,
        ctx: &ActionCtx,
        request: HttpRequest,
    ) -> Result<Value, ConnectorError> {
        let resp = ctx.http.send(request.timeout(ctx.timeout)).await?;
        if resp.is_success() {
            return resp.json();
        }
        Err(map_error(&resp, ctx.now))
    }

    /// Like `send`, but a 404 becomes `None`.
    pub(super) async fn send_optional(
        &self,
        ctx: &ActionCtx,
        request: HttpRequest,
    ) -> Result<Option<Value>, ConnectorError> {
        match self.send(ctx, request).await {
            Ok(v) => Ok(Some(v)),
            Err(ConnectorError::NotFound { .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

/// Appends `?trigger=` (no automation) when workflows must be skipped — prevents write-back loops.
/// Only API tokens may send it (`openapi.yaml` `RecordWriteTrigger`).
pub(super) fn with_trigger(request: HttpRequest, skip_workflows: bool) -> HttpRequest {
    if skip_workflows {
        request.query(&[("trigger", "")])
    } else {
        request
    }
}

/// Maps Appblocks error responses onto the connector error taxonomy.
pub(super) fn map_error(resp: &HttpResponse, now: DateTime<Utc>) -> ConnectorError {
    let body = resp.json().unwrap_or(Value::Null);
    let message = body
        .get("error")
        .and_then(Value::as_str)
        .map_or_else(|| resp.text(), str::to_owned);
    let code = body.get("code").and_then(Value::as_str).unwrap_or_default();
    let detail = if code.is_empty() {
        message.clone()
    } else {
        format!("{code}: {message}")
    };
    if resp.status == 429 {
        let retry_after_secs = resp
            .header("retry-after")
            .and_then(|v| v.trim().parse().ok())
            .or_else(|| {
                // The org's daily token cap resets at UTC midnight.
                (resp.header("x-ratelimit-remaining") == Some("0"))
                    .then(|| secs_until_utc_midnight(now))
            });
        return ConnectorError::RateLimited {
            retry_after_secs: retry_after_secs.or(Some(60)),
            message: detail,
        };
    }
    if resp.status == 409 || code == "DUPLICATE_VALUE" {
        return ConnectorError::invalid(format!("appblocks.duplicate: {detail}"));
    }
    if message.to_lowercase().contains("locked") {
        return ConnectorError::invalid(format!("appblocks.record_locked: {detail}"));
    }
    match ConnectorError::from_http(resp) {
        ConnectorError::InvalidInput { field, .. } => ConnectorError::InvalidInput {
            field,
            message: detail,
        },
        ConnectorError::NotFound { .. } => ConnectorError::NotFound { message: detail },
        ConnectorError::AuthExpired { .. } => ConnectorError::AuthExpired {
            message: format!("Appblocks rejected the API token: {detail}"),
        },
        ConnectorError::Permanent { status, .. } => ConnectorError::Permanent {
            status,
            message: detail,
        },
        other @ (ConnectorError::Transient { .. }
        | ConnectorError::RateLimited { .. }
        | ConnectorError::Unsupported { .. }
        | ConnectorError::Coded { .. }) => other,
    }
}

fn secs_until_utc_midnight(now: DateTime<Utc>) -> u64 {
    let tomorrow = now
        .date_naive()
        .succ_opt()
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|n| n.and_utc());
    tomorrow
        .map(|t| t - now)
        .and_then(|d: Duration| u64::try_from(d.num_seconds()).ok())
        .unwrap_or(3_600)
        .max(1)
}

/// Constant-time string comparison for shared secrets.
pub(super) fn secure_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn module_and_id_validation() {
        assert!(check_module("job_visits").is_ok());
        assert!(check_module("jobs/../x").is_err());
        assert!(check_id("record_id", "01HZX8P6M0Q4T2V9B3C7D5E1F0").is_ok());
        assert!(check_id("record_id", "a/b").is_err());
    }

    #[test]
    fn daily_cap_waits_until_midnight() {
        let now = Utc.with_ymd_and_hms(2026, 1, 1, 23, 0, 0).single().unwrap();
        assert_eq!(secs_until_utc_midnight(now), 3_600);
        assert!(secure_eq("abc", "abc") && !secure_eq("abc", "abd") && !secure_eq("ab", "abc"));
    }
}
