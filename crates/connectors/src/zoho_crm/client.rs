//! Zoho CRM API v8 access: `Zoho-oauthtoken` auth with one refresh-and-retry on 401,
//! per-data-centre API domain, and Zoho's error envelope mapping.

use weft_connector_sdk::prelude::*;

/// Default API domain (US data centre) when the connection has none.
pub(super) const DEFAULT_API_DOMAIN: &str = "https://www.zohoapis.com";

/// API base: `{api_domain}/crm/v8`, from the connection config.
pub(super) fn base(ctx: &ActionCtx) -> Result<String, ConnectorError> {
    let domain = ctx
        .conn()?
        .config()
        .get("api_domain")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(DEFAULT_API_DOMAIN)
        .trim_end_matches('/')
        .to_owned();
    Ok(format!("{domain}/crm/v8"))
}

/// Module API names are identifiers (`Deals`, `Custom_Module1`): reject anything else (path injection).
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

/// Zoho record IDs are numeric strings.
pub(super) fn check_id(field: &str, id: &str) -> Result<String, ConnectorError> {
    let id = id.trim();
    if !id.is_empty() && id.len() <= 32 && id.chars().all(|c| c.is_ascii_digit()) {
        Ok(id.to_owned())
    } else {
        Err(ConnectorError::invalid_field(
            field,
            format!("`{id}` is not a Zoho record ID"),
        ))
    }
}

/// Sends a request built with the current access token. On 401 the token is refreshed once
/// and the request retried. Returns the JSON body (`null` for 204 No Content).
pub(super) async fn send(
    ctx: &ActionCtx,
    build: impl Fn(&str) -> HttpRequest + Send + Sync,
) -> Result<Value, ConnectorError> {
    let conn = ctx.conn()?;
    let token = conn.access_token().await?;
    let mut resp = ctx.http.send(build(&token).timeout(ctx.timeout)).await?;
    // A missing permission is also a 401, but a new token does not help: the connection must be signed in
    // again with the scope.
    if resp.status == 401 && missing_scope(&resp) {
        return Err(ConnectorError::AuthExpired {
            message: format!(
                "Zoho refused: the connection lacks a permission; reconnect it ({})",
                resp.text()
            ),
        });
    }
    if resp.status == 401 {
        let fresh = conn.force_refresh().await?;
        resp = ctx.http.send(build(&fresh).timeout(ctx.timeout)).await?;
        if resp.status == 401 {
            return Err(ConnectorError::AuthExpired {
                message: format!("Zoho rejected the refreshed token: {}", resp.text()),
            });
        }
    }
    // 204: nothing found; 304: nothing modified since `If-Modified-Since`.
    if resp.status == 204 || resp.status == 304 {
        return Ok(Value::Null);
    }
    if resp.is_success() {
        return resp.json();
    }
    Err(map_error(&resp))
}

fn missing_scope(resp: &HttpResponse) -> bool {
    resp.json().ok().is_some_and(|b: Value| {
        matches!(
            b.get("code").and_then(Value::as_str),
            Some("OAUTH_SCOPE_MISMATCH" | "NO_PERMISSION")
        )
    })
}

/// A request with the Zoho auth header.
pub(super) fn request(method: Method, url: &str, token: &str) -> HttpRequest {
    HttpRequest::new(method, url)
        .header("Authorization", format!("Zoho-oauthtoken {token}"))
        .header("Accept", "application/json")
}

/// Maps one Zoho `{code, message, details, status}` object to a connector error.
pub(super) fn error_from_code(
    code: &str,
    message: &str,
    details: &Value,
    http_status: Option<u16>,
) -> ConnectorError {
    let field = details
        .get("api_name")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let text = format!("{code}: {message}");
    match code {
        "INVALID_TOKEN"
        | "AUTHENTICATION_FAILURE"
        | "OAUTH_SCOPE_MISMATCH"
        | "INVALID_OAUTHTOKEN" => ConnectorError::AuthExpired { message: text },
        "TOO_MANY_REQUESTS" | "LIMIT_REACHED" | "API_LIMIT_EXCEEDED" => {
            ConnectorError::RateLimited {
                retry_after_secs: Some(60),
                message: text,
            }
        }
        "INTERNAL_ERROR" | "INTERNAL_SERVER_ERROR" => ConnectorError::Transient { message: text },
        "INVALID_URL_PATTERN" | "INVALID_MODULE" | "NO_PERMISSION" | "FEATURE_NOT_SUPPORTED" => {
            ConnectorError::Permanent {
                status: http_status,
                message: text,
            }
        }
        _ => ConnectorError::InvalidInput {
            field,
            message: text,
        },
    }
}

/// Maps a non-2xx Zoho response.
pub(super) fn map_error(resp: &HttpResponse) -> ConnectorError {
    let body = resp.json().unwrap_or(Value::Null);
    if resp.status == 429 {
        let retry = resp
            .header("retry-after")
            .and_then(|v| v.trim().parse().ok())
            .or(Some(60));
        return ConnectorError::RateLimited {
            retry_after_secs: retry,
            message: resp.text(),
        };
    }
    // A single-record write that fails comes back as 400 with the error in `data[0]`.
    let entry = if body.get("code").is_some() {
        &body
    } else {
        body.get("data")
            .and_then(|d| d.get(0))
            .unwrap_or(&Value::Null)
    };
    match entry.get("code").and_then(Value::as_str) {
        Some(code) if resp.status < 500 => {
            let message = entry
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or_default();
            error_from_code(
                code,
                message,
                entry.get("details").unwrap_or(&Value::Null),
                Some(resp.status),
            )
        }
        Some(_) | None => ConnectorError::from_http(resp),
    }
}

/// The first per-record result in a write response (`data[0]`), or its mapped error.
pub(super) fn first_result(body: &Value) -> Result<Value, ConnectorError> {
    let item = body
        .get("data")
        .and_then(Value::as_array)
        .and_then(|d| d.first())
        .cloned()
        .ok_or_else(|| ConnectorError::permanent(format!("unexpected Zoho response: {body}")))?;
    if item.get("status").and_then(Value::as_str) == Some("error") {
        let code = item.get("code").and_then(Value::as_str).unwrap_or("ERROR");
        let message = item
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or_default();
        return Err(error_from_code(
            code,
            message,
            item.get("details").unwrap_or(&Value::Null),
            None,
        ));
    }
    Ok(item)
}

/// Constant-time string comparison for shared secrets.
pub(super) fn secure_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn per_record_errors_map_to_classes() {
        let dup = json!({"data": [{"code": "DUPLICATE_DATA", "status": "error", "message": "duplicate", "details": {"api_name": "Email"}}]});
        assert!(
            matches!(first_result(&dup), Err(ConnectorError::InvalidInput { field: Some(f), .. }) if f == "Email")
        );
        let ok =
            json!({"data": [{"code": "SUCCESS", "status": "success", "details": {"id": "1"}}]});
        assert_eq!(first_result(&ok).unwrap()["details"]["id"], "1");
        assert!(check_id("record_id", "4876876000000624001").is_ok());
        assert!(check_id("record_id", "12/../x").is_err());
        assert!(check_module("Deals").is_ok() && check_module("Deals?x").is_err());
    }
}
