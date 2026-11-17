//! Use cases: webhook ingress (`/hooks/{token}`) and app-event ingress (`/hooks/app/{token}`).

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Utc};
use serde_json::{Map, Value, json};
use weft_connector_sdk::{ConnectorError, EventItem, InboundRequest};
use weft_domain::{
    error_codes,
    flow::TriggerNode,
    ids::ExecutionId,
    model::{ExecutionMode, Flow, FlowVersion, RegistrationKind, TriggerRegistration},
};

use crate::{
    AppError, Deps,
    executions::start::new_execution,
    ports::{NewExecution, NewTriggerEvent},
    runner::action::{action_ctx, load_connection},
};

/// Maximum webhook body size (doc 01 §4).
pub const MAX_BODY_BYTES: usize = 5 * 1024 * 1024;
const MAX_SPLIT: usize = 100;
const HIDDEN_HEADERS: [&str; 3] = ["authorization", "cookie", "proxy-authorization"];

/// A raw inbound HTTP request.
#[derive(Debug, Clone, Default)]
pub struct InboundHttp {
    pub method: String,
    /// Lower-cased names.
    pub headers: BTreeMap<String, String>,
    pub query: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

/// What ingress did.
#[derive(Debug, Clone, PartialEq)]
pub struct IngressResult {
    /// `accepted`, `ignored` or `sample_captured`.
    pub status: &'static str,
    pub executions: Vec<ExecutionId>,
    /// An immediate response body requested by the connector (handshakes).
    pub response: Option<Value>,
}

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

fn visible_headers(headers: &BTreeMap<String, String>) -> Map<String, Value> {
    headers
        .iter()
        .filter(|(k, _)| !HIDDEN_HEADERS.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), Value::String(v.clone())))
        .collect()
}

async fn load_target(deps: &Deps, token: &str) -> Result<(TriggerRegistration, Flow), AppError> {
    let reg = deps
        .store
        .get_registration_by_token(token)
        .await?
        .ok_or_else(|| AppError::not_found(error_codes::WEBHOOK_UNKNOWN, "Unknown webhook"))?;
    let flow = deps
        .store
        .get_flow_system(reg.flow_id)
        .await?
        .ok_or_else(|| AppError::not_found(error_codes::WEBHOOK_UNKNOWN, "Unknown webhook"))?;
    Ok((reg, flow))
}

async fn published(deps: &Deps, flow: &Flow) -> Result<Option<FlowVersion>, AppError> {
    match flow.published_version_id {
        Some(v) => deps.store.get_version(v).await,
        None => Ok(None),
    }
}

/// Rejects the request when the webhook requires a secret header and it is missing or wrong.
fn check_secret(
    cfg: &weft_domain::flow::WebhookTrigger,
    req: &InboundHttp,
    reg: &TriggerRegistration,
) -> Result<(), AppError> {
    let Some(header) = &cfg.secret_header else {
        return Ok(());
    };
    let given = req
        .headers
        .get(&header.to_lowercase())
        .map(String::as_str)
        .unwrap_or_default();
    if constant_eq(given, &reg.secret) {
        Ok(())
    } else {
        Err(AppError::new(
            crate::ErrorKind::Unauthorized,
            error_codes::WEBHOOK_UNAUTHORIZED,
            "Missing or wrong webhook secret",
        ))
    }
}

/// Webhook trigger ingress.
pub async fn ingest_webhook(
    deps: &Deps,
    token: &str,
    req: &InboundHttp,
) -> Result<IngressResult, AppError> {
    if req.body.len() > MAX_BODY_BYTES {
        return Err(AppError::validation(
            error_codes::WEBHOOK_TOO_LARGE,
            "The request body is larger than 5 MB",
        ));
    }
    let (reg, flow) = load_target(deps, token).await?;
    let now = deps.clock.now();
    let content_type = req
        .headers
        .get("content-type")
        .map(String::as_str)
        .unwrap_or_default();
    let body = parse_body(content_type, &req.body);
    let data = json!({
        "method": req.method,
        "headers": visible_headers(&req.headers),
        "query": req.query,
        "body": body,
        "received_at": now,
    });
    let listening = reg.listening_until.is_some_and(|u| u > now);
    if listening {
        deps.store.set_sample(reg.id, &data, now).await?;
        deps.store.set_listening(reg.id, None, now).await?;
    }
    let version = published(deps, &flow).await?;
    let (Some(version), true) = (
        version,
        reg.is_active && reg.kind == RegistrationKind::Webhook,
    ) else {
        let status = if listening {
            "sample_captured"
        } else {
            "ignored"
        };
        return Ok(IngressResult {
            status,
            executions: Vec::new(),
            response: None,
        });
    };
    let TriggerNode::Webhook(cfg) = &version.definition.trigger else {
        return Ok(IngressResult {
            status: "ignored",
            executions: Vec::new(),
            response: None,
        });
    };
    check_secret(cfg, req, &reg)?;
    let idem_header = cfg
        .idempotency_header
        .clone()
        .unwrap_or_else(|| "idempotency-key".into())
        .to_lowercase();
    let idem = req.headers.get(&idem_header).cloned();
    let bodies: Vec<Value> = match (&data["body"], cfg.split_array) {
        (Value::Array(items), true) => items.iter().take(MAX_SPLIT).cloned().collect(),
        (other, _) => vec![other.clone()],
    };
    let items: Vec<NewExecution> = bodies
        .into_iter()
        .enumerate()
        .map(|(i, b)| {
            let mut d = data.clone();
            d["body"] = b;
            let event = NewTriggerEvent {
                id: deps.new_event_id(),
                registration_id: reg.id,
                dedupe_key: idem.as_ref().map(|k| format!("{k}:{i}")),
                payload: d.clone(),
            };
            new_execution(
                deps,
                flow.org_id,
                flow.id,
                version.id,
                ExecutionMode::Live,
                d,
                Some(event),
                now,
                None,
            )
        })
        .collect();
    let executions = deps
        .store
        .create_executions(flow.org_id, flow.id, &items, now)
        .await?;
    deps.wake.notify_one();
    Ok(IngressResult {
        status: "accepted",
        executions,
        response: None,
    })
}

/// App trigger ingress: the connector verifies and parses the request (and may fetch records).
pub async fn ingest_app_event(
    deps: &Deps,
    token: &str,
    req: &InboundHttp,
) -> Result<IngressResult, AppError> {
    if req.body.len() > MAX_BODY_BYTES {
        return Err(AppError::validation(
            error_codes::WEBHOOK_TOO_LARGE,
            "The request body is larger than 5 MB",
        ));
    }
    let (reg, flow) = load_target(deps, token).await?;
    let now = deps.clock.now();
    // Use the published trigger when there is one, otherwise the draft (sample capture while building).
    let version = published(deps, &flow).await?;
    let trigger = version
        .as_ref()
        .map_or(&flow.draft, |v| &v.definition)
        .trigger
        .clone();
    let TriggerNode::App(app) = &trigger else {
        return Ok(IngressResult {
            status: "ignored",
            executions: Vec::new(),
            response: None,
        });
    };
    let connector = deps
        .connectors
        .get(&app.connector)
        .cloned()
        .ok_or_else(|| AppError::not_found(error_codes::CONNECTOR_NOT_FOUND, "Unknown app"))?;
    let connection = load_connection(deps, reg.org_id, app.connection_id.as_deref())
        .await
        .map_err(|e| AppError::validation(error_codes::CONNECTION_NOT_FOUND, e.to_string()))?;
    let ctx = action_ctx(deps, reg.org_id, connection, String::new());
    let content_type = req
        .headers
        .get("content-type")
        .map(String::as_str)
        .unwrap_or_default();
    let inbound = InboundRequest {
        method: req.method.clone(),
        headers: req.headers.clone(),
        query: req.query.clone(),
        body: parse_body(content_type, &req.body),
    };
    let batch = connector
        .handle_event(&ctx, &app.trigger, &app.input, &inbound, &reg.secret)
        .await
        .map_err(|e| match e {
            ConnectorError::InvalidInput { message, .. } => AppError::new(
                crate::ErrorKind::Unauthorized,
                error_codes::WEBHOOK_UNAUTHORIZED,
                message,
            ),
            other @ (ConnectorError::Transient { .. }
            | ConnectorError::RateLimited { .. }
            | ConnectorError::AuthExpired { .. }
            | ConnectorError::NotFound { .. }
            | ConnectorError::Permanent { .. }
            | ConnectorError::Unsupported { .. }) => AppError::new(
                crate::ErrorKind::BadGateway,
                other.code(),
                other.to_string(),
            ),
        })?;
    let listening = reg.listening_until.is_some_and(|u| u > now);
    if listening && let Some(first) = batch.items.first() {
        deps.store.set_sample(reg.id, &first.data, now).await?;
        deps.store.set_listening(reg.id, None, now).await?;
    }
    let active_version = version.filter(|_| reg.is_active);
    let Some(version) = active_version else {
        let status = if listening {
            "sample_captured"
        } else {
            "ignored"
        };
        return Ok(IngressResult {
            status,
            executions: Vec::new(),
            response: batch.response,
        });
    };
    let executions = start_items(
        deps,
        &reg,
        &flow,
        &version,
        &batch.items,
        app.debounce_secs,
        ExecutionMode::Live,
        now,
    )
    .await?;
    Ok(IngressResult {
        status: "accepted",
        executions,
        response: batch.response,
    })
}

/// Creates executions for trigger items (with optional debounce per record).
#[allow(clippy::too_many_arguments)]
pub(crate) async fn start_items(
    deps: &Deps,
    reg: &TriggerRegistration,
    flow: &Flow,
    version: &FlowVersion,
    items: &[EventItem],
    debounce_secs: Option<u32>,
    mode: ExecutionMode,
    now: DateTime<Utc>,
) -> Result<Vec<ExecutionId>, AppError> {
    let new: Vec<NewExecution> = items
        .iter()
        .map(|item| {
            let (dedupe, run_at) = match (debounce_secs.filter(|s| *s > 0), &item.record_id) {
                (Some(secs), Some(record)) => {
                    let window = i64::from(secs);
                    let bucket = now.timestamp().div_euclid(window) + 1;
                    let run_at = DateTime::from_timestamp(bucket * window, 0).unwrap_or(now);
                    (
                        format!("rec:{record}:{bucket}"),
                        run_at.max(now + Duration::seconds(1)),
                    )
                }
                _ => (item.dedupe_key.clone(), now),
            };
            let event = NewTriggerEvent {
                id: deps.new_event_id(),
                registration_id: reg.id,
                dedupe_key: Some(dedupe),
                payload: item.data.clone(),
            };
            new_execution(
                deps,
                flow.org_id,
                flow.id,
                version.id,
                mode,
                item.data.clone(),
                Some(event),
                run_at,
                None,
            )
        })
        .collect();
    let ids = deps
        .store
        .create_executions(flow.org_id, flow.id, &new, now)
        .await?;
    if !ids.is_empty() {
        deps.wake.notify_one();
    }
    Ok(ids)
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
