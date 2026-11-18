//! Cross-cutting HTTP behaviour: every error is problem+json (including axum's own rejections and unknown routes),
//! request logs name a path without capability tokens, and webhook ingress is rate limited per URL.

use std::{collections::HashMap, sync::Mutex};

use axum::{
    Json,
    body::to_bytes,
    extract::{Request, State},
    http::{HeaderValue, StatusCode, Uri, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use serde_json::json;
use weft_app::limits::TokenBucket;

use crate::state::ApiState;

/// The path to log: webhook tokens are credentials, and query strings carry OAuth codes and states.
#[must_use]
pub fn log_path(uri: &Uri) -> String {
    let path = uri.path();
    match path.strip_prefix("/hooks/") {
        Some(rest) if rest.starts_with("app/") => "/hooks/app/…".to_owned(),
        Some(_) => "/hooks/…".to_owned(),
        None => path.to_owned(),
    }
}

/// Turns error responses that are not problem+json (axum's rejections of bad JSON, a wrong query, a body over the
/// limit, an unknown route or method) into problem+json with a stable `http.<status>` code.
pub async fn problem_errors(request: Request, next: Next) -> Response {
    let browser_page = request.uri().path().starts_with("/oauth/");
    let response = next.run(request).await;
    let status = response.status();
    let is_problem = response
        .headers()
        .get(header::CONTENT_TYPE)
        .is_some_and(|v| v.as_bytes().starts_with(b"application/problem+json"));
    if browser_page || is_problem || !(status.is_client_error() || status.is_server_error()) {
        return response;
    }
    let text = to_bytes(response.into_body(), 4096)
        .await
        .unwrap_or_default();
    let detail = String::from_utf8_lossy(&text)
        .trim()
        .chars()
        .take(300)
        .collect::<String>();
    let body = json!({
        "type": "about:blank",
        "title": status.canonical_reason().unwrap_or("Error"),
        "status": status.as_u16(),
        "code": format!("http.{}", status.as_u16()),
        "detail": if detail.is_empty() { status.canonical_reason().unwrap_or("Error").to_owned() } else { detail },
    });
    (
        status,
        [(header::CONTENT_TYPE, "application/problem+json")],
        Json(body),
    )
        .into_response()
}

/// Requests per minute one webhook URL may receive (each may start up to 100 runs).
pub const WEBHOOK_PER_MINUTE: u32 = 600;
/// URLs tracked at once; past this the table starts again (limits are a safeguard, not accounting).
const TRACKED: usize = 50_000;

/// Per-URL token buckets for webhook ingress (per process).
#[derive(Debug, Default)]
pub struct WebhookLimiter {
    buckets: Mutex<HashMap<String, TokenBucket>>,
}

impl WebhookLimiter {
    /// `Err(seconds)` until the next request is allowed.
    pub fn take(&self, token: &str, now: DateTime<Utc>) -> Result<(), u64> {
        let Ok(mut buckets) = self.buckets.lock() else {
            return Ok(());
        };
        if buckets.len() >= TRACKED && !buckets.contains_key(token) {
            buckets.clear();
        }
        buckets
            .entry(token.to_owned())
            .or_insert_with(|| TokenBucket::new(WEBHOOK_PER_MINUTE, now))
            .take(now)
    }
}

/// Refuses webhook requests over the per-URL rate with 429 and `Retry-After`.
pub async fn limit_webhooks(State(s): State<ApiState>, request: Request, next: Next) -> Response {
    let token = request
        .uri()
        .path()
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_owned();
    match s.webhooks.take(&token, s.deps.clock.now()) {
        Ok(()) => next.run(request).await,
        Err(secs) => too_many(secs),
    }
}

fn too_many(retry_after_secs: u64) -> Response {
    let body = json!({
        "type": "about:blank",
        "title": "Too Many Requests",
        "status": 429,
        "code": "webhook.rate_limited",
        "detail": format!("This webhook received more than {WEBHOOK_PER_MINUTE} requests a minute; retry later"),
    });
    let mut response = (
        StatusCode::TOO_MANY_REQUESTS,
        [(header::CONTENT_TYPE, "application/problem+json")],
        Json(body),
    )
        .into_response();
    if let Ok(v) = HeaderValue::from_str(&retry_after_secs.max(1).to_string()) {
        response.headers_mut().insert(header::RETRY_AFTER, v);
    }
    response
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone as _;

    use super::*;

    #[test]
    fn webhook_urls_are_limited_one_by_one() {
        let limiter = WebhookLimiter::default();
        let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        for _ in 0..WEBHOOK_PER_MINUTE {
            assert!(limiter.take("a", now).is_ok());
        }
        assert!(limiter.take("a", now).is_err(), "the burst is used up");
        assert!(
            limiter.take("b", now).is_ok(),
            "another URL has its own budget"
        );
        assert!(
            limiter
                .take("a", now + chrono::Duration::seconds(1))
                .is_ok(),
            "it refills"
        );
    }

    #[test]
    fn logged_paths_hide_webhook_tokens_and_queries() {
        let uri: Uri = "/hooks/abc123?x=1".parse().unwrap();
        assert_eq!(log_path(&uri), "/hooks/…");
        let uri: Uri = "/oauth/callback?code=secret&state=s".parse().unwrap();
        assert_eq!(log_path(&uri), "/oauth/callback");
    }
}
