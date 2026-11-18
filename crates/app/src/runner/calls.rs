//! The HTTP calls a step attempt makes, recorded for the run's detailed log. While a step runs, every action
//! context built in its task wraps its HTTP sender in a recorder (a task-local scope, so connectors and the sync
//! layer need no extra parameter). A call is logged with its method, its URL without credentials or secret query
//! values, status, time and sizes, and the start of the answer when the app refused. Headers and request bodies
//! are never logged. Requests our own limits held back were never sent and are not logged as calls.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU32, Ordering},
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use weft_connector_sdk::{ConnectorError, HttpRequest, HttpResponse, HttpSender};

use crate::{
    Deps,
    ports::{LogKind, LogLevel, LogRecord},
};

/// Calls logged per attempt; more are only counted (a step looping over many records).
const MAX_CALLS: usize = 50;
/// Bytes of an error answer kept.
const EXCERPT_BYTES: usize = 2048;
/// Query parameters whose values are never logged.
const SECRET_PARAMS: [&str; 14] = [
    "token",
    "access_token",
    "refresh_token",
    "id_token",
    "api_key",
    "apikey",
    "key",
    "secret",
    "client_secret",
    "password",
    "signature",
    "sig",
    "code",
    "auth",
];

tokio::task_local! {
    static CALLS: Arc<CallLog>;
}

/// One HTTP call.
#[derive(Debug, Clone, PartialEq)]
pub struct HttpCall {
    pub at: DateTime<Utc>,
    pub method: &'static str,
    pub url: String,
    /// The answer's status, or `None` when no answer came (network error, timeout).
    pub status: Option<u16>,
    pub error: Option<String>,
    pub duration_ms: i64,
    pub request_bytes: usize,
    pub response_bytes: usize,
    pub excerpt: Option<String>,
}

/// The calls of one attempt.
#[derive(Debug, Default)]
pub struct CallLog {
    calls: Mutex<Vec<HttpCall>>,
    not_logged: AtomicU32,
}

impl CallLog {
    fn push(&self, call: HttpCall) {
        if let Ok(mut calls) = self.calls.lock()
            && calls.len() < MAX_CALLS
        {
            calls.push(call);
            return;
        }
        self.not_logged.fetch_add(1, Ordering::Relaxed);
    }

    /// The attempt's log lines: one per call, plus a note when some were only counted.
    #[must_use]
    pub fn records(&self, deps: &Deps, step: &str, attempt: u16) -> Vec<LogRecord> {
        let calls = self.calls.lock().map(|c| c.clone()).unwrap_or_default();
        let mut out: Vec<LogRecord> = calls
            .iter()
            .map(|c| record(deps, step, attempt, c))
            .collect();
        let more = self.not_logged.load(Ordering::Relaxed);
        if more > 0 {
            out.push(LogRecord {
                id: deps.ids.uuid(),
                execution_id: weft_domain::ids::ExecutionId::from_uuid(uuid::Uuid::nil()),
                step_key: Some(step.to_owned()),
                attempt: Some(attempt),
                level: LogLevel::Info,
                kind: LogKind::Http,
                message: format!("{more} more HTTP calls were made but not logged"),
                details: Some(json!({"not_logged": more})),
                at: calls.last().map_or_else(|| deps.clock.now(), |c| c.at),
            });
        }
        out
    }
}

fn record(deps: &Deps, step: &str, attempt: u16, c: &HttpCall) -> LogRecord {
    let (level, outcome) = match (c.status, &c.error) {
        (Some(s), _) if s >= 500 || s == 429 => (LogLevel::Warn, s.to_string()),
        (Some(s), _) if s >= 400 => (LogLevel::Error, s.to_string()),
        (Some(s), _) => (LogLevel::Info, s.to_string()),
        (None, Some(e)) => (LogLevel::Warn, format!("no answer ({e})")),
        (None, None) => (LogLevel::Warn, "no answer".to_owned()),
    };
    LogRecord {
        id: deps.ids.uuid(),
        execution_id: weft_domain::ids::ExecutionId::from_uuid(uuid::Uuid::nil()),
        step_key: Some(step.to_owned()),
        attempt: Some(attempt),
        level,
        kind: LogKind::Http,
        message: format!("{} {} → {outcome} in {} ms", c.method, c.url, c.duration_ms),
        details: Some(json!({
            "method": c.method,
            "url": c.url,
            "status": c.status,
            "error": c.error,
            "duration_ms": c.duration_ms,
            "request_bytes": c.request_bytes,
            "response_bytes": c.response_bytes,
            "response_excerpt": c.excerpt,
        })),
        at: c.at,
    }
}

/// Runs `work` with its HTTP calls recorded into `log`.
pub async fn recording<F: Future>(log: Arc<CallLog>, work: F) -> F::Output {
    CALLS.scope(log, work).await
}

/// `inner`, recording its calls when a step attempt is being recorded in this task.
pub(crate) fn wrap(deps: &Deps, inner: Arc<dyn HttpSender>) -> Arc<dyn HttpSender> {
    match CALLS.try_with(Arc::clone) {
        Ok(log) => Arc::new(Recorder {
            deps: deps.clone(),
            inner,
            log,
        }),
        Err(_) => inner,
    }
}

struct Recorder {
    deps: Deps,
    inner: Arc<dyn HttpSender>,
    log: Arc<CallLog>,
}

impl std::fmt::Debug for Recorder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Recorder").finish_non_exhaustive()
    }
}

#[async_trait]
impl HttpSender for Recorder {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, ConnectorError> {
        let at = self.deps.clock.now();
        let method = request.method.as_str();
        let url = redact_url(&request.url);
        let request_bytes = request.body.as_ref().map_or(0, Vec::len);
        let result = self.inner.send(request).await;
        let duration_ms = (self.deps.clock.now() - at).num_milliseconds().max(0);
        let (status, error, response_bytes, excerpt) = match &result {
            // Held back by our own limits: never sent (the run's log notes the wait instead).
            Err(ConnectorError::Throttled { message, .. })
                if message.starts_with("connection.limit") =>
            {
                return result;
            }
            Ok(r) => (
                Some(r.status),
                None,
                r.body.len(),
                (r.status >= 400).then(|| excerpt(&r.body)),
            ),
            Err(e) => (None, Some(e.to_string()), 0, None),
        };
        self.log.push(HttpCall {
            at,
            method,
            url,
            status,
            error,
            duration_ms,
            request_bytes,
            response_bytes,
            excerpt,
        });
        result
    }
}

fn excerpt(body: &[u8]) -> String {
    let cut = &body[..body.len().min(EXCERPT_BYTES)];
    let mut text = String::from_utf8_lossy(cut).into_owned();
    if body.len() > EXCERPT_BYTES {
        text.push('…');
    }
    text
}

/// The URL without user info, fragment or secret query values (`?token=***`).
#[must_use]
pub fn redact_url(raw: &str) -> String {
    let Ok(mut url) = url::Url::parse(raw) else {
        return raw.split(['?', '#']).next().unwrap_or_default().to_owned();
    };
    let _ = url.set_username("");
    let _ = url.set_password(None);
    url.set_fragment(None);
    let pairs: Vec<(String, String)> = url
        .query_pairs()
        .map(|(k, v)| {
            let secret = SECRET_PARAMS.contains(&k.to_ascii_lowercase().as_str());
            (
                k.into_owned(),
                // `***` needs no percent-encoding, so the logged URL stays readable.
                if secret {
                    "***".to_owned()
                } else {
                    v.into_owned()
                },
            )
        })
        .collect();
    if pairs.is_empty() {
        url.set_query(None);
    } else {
        url.query_pairs_mut().clear().extend_pairs(pairs);
    }
    url.to_string()
}

/// The URL as viewers see it: scheme and host only (paths and queries carry record data).
#[must_use]
pub fn origin_only(url: &str) -> String {
    url::Url::parse(url).map_or_else(|_| String::new(), |u| u.origin().ascii_serialization())
}

/// Details of an HTTP log line as viewers see them.
#[must_use]
pub fn viewer_details(details: &Value) -> Value {
    json!({
        "method": details.get("method"),
        "url": details.get("url").and_then(Value::as_str).map(origin_only),
        "status": details.get("status"),
        "duration_ms": details.get("duration_ms"),
    })
}

/// The message of an HTTP log line as viewers see it (from their `viewer_details`).
#[must_use]
pub fn viewer_http_message(details: Option<&Value>) -> String {
    let get = |k: &str| {
        details
            .and_then(|d| d.get(k))
            .cloned()
            .unwrap_or(Value::Null)
    };
    let status = get("status");
    let outcome = status
        .as_u64()
        .map_or_else(|| "no answer".to_owned(), |s| s.to_string());
    format!(
        "{} {} → {outcome} in {} ms",
        get("method").as_str().unwrap_or("?"),
        get("url").as_str().unwrap_or(""),
        get("duration_ms").as_i64().unwrap_or(0)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logged_urls_keep_no_credentials_or_secret_query_values() {
        assert_eq!(
            redact_url(
                "https://user:pw@api.example.com/v2/Leads/42?fields=Name&access_token=abc&KEY=k#frag"
            ),
            "https://api.example.com/v2/Leads/42?fields=Name&access_token=***&KEY=***"
        );
        assert_eq!(
            redact_url("https://api.example.com/x"),
            "https://api.example.com/x"
        );
        assert_eq!(redact_url("not a url?token=x"), "not a url");
        assert_eq!(
            origin_only("https://api.example.com/v2/Leads/42?x=1"),
            "https://api.example.com"
        );
        assert_eq!(excerpt(&[b'a'; 3000]).chars().count(), EXCERPT_BYTES + 1);
    }
}
