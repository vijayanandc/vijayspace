//! The HTTP abstraction connectors use. The real sender (with the SSRF guard) lives in `weft-infra`.

use std::time::Duration;

use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;

use crate::error::ConnectorError;

/// HTTP method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl Method {
    /// Parses `GET`, `post`, … (defaults to an error for anything else).
    pub fn parse(s: &str) -> Result<Self, ConnectorError> {
        match s.to_ascii_uppercase().as_str() {
            "GET" => Ok(Self::Get),
            "POST" => Ok(Self::Post),
            "PUT" => Ok(Self::Put),
            "PATCH" => Ok(Self::Patch),
            "DELETE" => Ok(Self::Delete),
            other => Err(ConnectorError::invalid_field("method", format!("unsupported method `{other}`"))),
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
        }
    }
}

/// An outgoing request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
    pub timeout: Option<Duration>,
}

impl HttpRequest {
    #[must_use]
    pub fn new(method: Method, url: impl Into<String>) -> Self {
        Self { method, url: url.into(), headers: Vec::new(), body: None, timeout: None }
    }

    #[must_use]
    pub fn get(url: impl Into<String>) -> Self {
        Self::new(Method::Get, url)
    }

    #[must_use]
    pub fn post(url: impl Into<String>) -> Self {
        Self::new(Method::Post, url)
    }

    #[must_use]
    pub fn header(mut self, name: &str, value: impl Into<String>) -> Self {
        self.headers.push((name.to_owned(), value.into()));
        self
    }

    #[must_use]
    pub fn bearer(self, token: &str) -> Self {
        self.header("Authorization", format!("Bearer {token}"))
    }

    /// Appends query parameters (URL-encoded) to the URL.
    #[must_use]
    pub fn query(mut self, params: &[(&str, &str)]) -> Self {
        if params.is_empty() {
            return self;
        }
        let encoded: Vec<String> = params
            .iter()
            .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
            .collect();
        let sep = if self.url.contains('?') { '&' } else { '?' };
        self.url = format!("{}{sep}{}", self.url, encoded.join("&"));
        self
    }

    #[must_use]
    pub fn json(mut self, body: &Value) -> Self {
        self.body = Some(body.to_string().into_bytes());
        self.header("Content-Type", "application/json")
    }

    #[must_use]
    pub fn form(mut self, params: &[(&str, &str)]) -> Self {
        let encoded: Vec<String> = params
            .iter()
            .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
            .collect();
        self.body = Some(encoded.join("&").into_bytes());
        self.header("Content-Type", "application/x-www-form-urlencoded")
    }

    #[must_use]
    pub fn raw_body(mut self, body: Vec<u8>) -> Self {
        self.body = Some(body);
        self
    }

    #[must_use]
    pub const fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }
}

/// A received response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    #[must_use]
    pub const fn is_success(&self) -> bool {
        self.status >= 200 && self.status < 300
    }

    /// Case-insensitive header lookup.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    #[must_use]
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    /// Parses the body as JSON (empty body → `null`).
    pub fn json(&self) -> Result<Value, ConnectorError> {
        if self.body.iter().all(u8::is_ascii_whitespace) {
            return Ok(Value::Null);
        }
        serde_json::from_slice(&self.body)
            .map_err(|e| ConnectorError::permanent(format!("invalid JSON from the app (HTTP {}): {e}", self.status)))
    }

    /// Returns the JSON body for 2xx, or the mapped error otherwise.
    pub fn json_ok(&self) -> Result<Value, ConnectorError> {
        if self.is_success() { self.json() } else { Err(ConnectorError::from_http(self)) }
    }
}

/// Sends HTTP requests. Network failures are `ConnectorError::Transient`; blocked
/// destinations (SSRF guard) are `ConnectorError::InvalidInput`.
#[async_trait]
pub trait HttpSender: Send + Sync {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, ConnectorError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_parameters_are_encoded_and_appended() {
        let r = HttpRequest::get("https://x.io/a?b=1").query(&[("q", "a b&c"), ("n", "2")]);
        assert_eq!(r.url, "https://x.io/a?b=1&q=a%20b%26c&n=2");
    }
}
