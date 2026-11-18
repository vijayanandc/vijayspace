//! `http_api` ("Any app (HTTP API)"): connect any app with a REST API, like Zoho Flow's custom apps.
//!
//! - **Connection:** a base URL and how requests are signed; credentials are encrypted in the connection, never in
//!   a flow (see `auth`).
//! - **Action:** "Make an API call" to a path under the base URL.
//! - **Trigger:** "New items" polls a list endpoint and starts a run per item not seen before. Apps that push
//!   events use the Webhook trigger.

mod auth;
mod manifest;

use std::time::Duration;

use weft_connector_sdk::{EventItem, PollResult, prelude::*};

use crate::util::{key_values, opt_bool, opt_f64, opt_str, req_str};

/// Item IDs remembered between polls (newest first) to tell new items from old.
const SEEN_MAX: usize = 1000;

/// The "Any app" connector.
#[derive(Debug)]
pub struct HttpApiConnector {
    manifest: ConnectorManifest,
    tokens: auth::TokenCache,
}

impl HttpApiConnector {
    #[must_use]
    pub fn new() -> Self {
        Self {
            manifest: manifest::manifest(),
            tokens: auth::TokenCache::default(),
        }
    }

    /// Sends a signed request; with OAuth client credentials a 401 fetches a new token and retries once.
    async fn send(
        &self,
        ctx: &ActionCtx,
        build: impl Fn() -> Result<HttpRequest, ConnectorError> + Send + Sync,
    ) -> Result<HttpResponse, ConnectorError> {
        let resp = ctx
            .http
            .send(auth::sign(ctx, &self.tokens, build()?).await?)
            .await?;
        let client_credentials =
            opt_str(ctx.conn()?.config(), "auth").as_deref() == Some("oauth2_client_credentials");
        if resp.status == 401 && client_credentials {
            self.tokens.forget(ctx.conn()?.id());
            return ctx
                .http
                .send(auth::sign(ctx, &self.tokens, build()?).await?)
                .await;
        }
        Ok(resp)
    }

    async fn api_call(&self, ctx: &ActionCtx, input: &Value) -> Result<Value, ConnectorError> {
        let method = Method::parse(&opt_str(input, "method").unwrap_or_else(|| "GET".into()))?;
        let base = base_url(ctx)?;
        let url = auth::url_for(&base, &req_str(input, "path")?)?;
        let timeout = opt_f64(input, "timeout_secs")
            .unwrap_or(30.0)
            .clamp(1.0, 120.0);
        let resp = self
            .send(ctx, || {
                let query = key_values(input, "query");
                let refs: Vec<(&str, &str)> = query
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str()))
                    .collect();
                let mut req = HttpRequest::new(method, url.clone())
                    .query(&refs)
                    .timeout(Duration::from_secs_f64(timeout));
                for (k, v) in key_values(input, "headers") {
                    req = req.header(&k, v);
                }
                if opt_bool(input, "send_idempotency_key").unwrap_or(false) {
                    req = req.header("Idempotency-Key", ctx.idempotency_key.clone());
                }
                Ok(with_body(req, method, input))
            })
            .await?;
        if opt_bool(input, "fail_on_error_status").unwrap_or(true) && !resp.is_success() {
            return Err(ConnectorError::from_http(&resp));
        }
        Ok(response_json(&resp))
    }
}

impl Default for HttpApiConnector {
    fn default() -> Self {
        Self::new()
    }
}

fn base_url(ctx: &ActionCtx) -> Result<String, ConnectorError> {
    opt_str(ctx.conn()?.config(), "base_url")
        .ok_or_else(|| ConnectorError::invalid_field("base_url", "The connection has no base URL"))
}

fn with_body(req: HttpRequest, method: Method, input: &Value) -> HttpRequest {
    let body = input.get("body").cloned().unwrap_or(Value::Null);
    if method == Method::Get || body.is_null() {
        return req;
    }
    match opt_str(input, "body_type").as_deref().unwrap_or("json") {
        "form" => {
            let fields = key_values(&json!({"b": body}), "b");
            let refs: Vec<(&str, &str)> = fields
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();
            req.form(&refs)
        }
        "text" => req
            .raw_body(
                body.as_str()
                    .map_or_else(|| body.to_string(), str::to_owned)
                    .into_bytes(),
            )
            .header("Content-Type", "text/plain"),
        "none" => req,
        _ => req.json(&body),
    }
}

/// `{status, headers, body}` (the body parsed as JSON when it is JSON).
fn response_json(resp: &HttpResponse) -> Value {
    let headers: Map<String, Value> = resp
        .headers
        .iter()
        .map(|(k, v)| (k.to_ascii_lowercase(), Value::String(v.clone())))
        .collect();
    let body = resp.json().unwrap_or_else(|_| Value::String(resp.text()));
    json!({"status": resp.status, "headers": headers, "body": body})
}

/// The list in `body` at a dotted path (`data.items`; empty = the body itself).
fn items_at<'a>(body: &'a Value, path: &str) -> Option<&'a Vec<Value>> {
    path.split('.')
        .filter(|p| !p.is_empty())
        .try_fold(body, |v, key| v.get(key))?
        .as_array()
}

fn item_id(item: &Value, field: &str) -> Option<String> {
    let value = item.get(field)?;
    value
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .or_else(|| value.as_number().map(ToString::to_string))
}

#[async_trait]
impl Connector for HttpApiConnector {
    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    /// GET, HEAD, PUT and DELETE are idempotent by HTTP's own rules; POST and PATCH are not.
    fn is_idempotent(&self, _action: &str, input: &Value) -> bool {
        let method = opt_str(input, "method")
            .unwrap_or_else(|| "GET".into())
            .to_ascii_uppercase();
        matches!(method.as_str(), "GET" | "HEAD" | "PUT" | "DELETE")
    }

    async fn execute(
        &self,
        ctx: &ActionCtx,
        action: &str,
        input: Value,
    ) -> Result<Value, ConnectorError> {
        match action {
            "api_call" => self.api_call(ctx, &input).await,
            other => Err(ConnectorError::unsupported(format!("action `{other}`"))),
        }
    }

    async fn test_connection(&self, ctx: &ActionCtx) -> Result<Value, ConnectorError> {
        let base = base_url(ctx)?;
        let path = opt_str(ctx.conn()?.config(), "test_path").unwrap_or_default();
        let url = auth::url_for(&base, &path)?;
        let resp = self
            .send(ctx, || {
                Ok(HttpRequest::new(Method::Get, url.clone()).timeout(ctx.timeout))
            })
            .await?;
        match resp.status {
            200..=299 => Ok(json!({"status": resp.status})),
            401 | 403 => Err(ConnectorError::AuthExpired {
                message: format!("the app refused the credentials (HTTP {})", resp.status),
            }),
            _ => Err(ConnectorError::from_http(&resp)),
        }
    }

    async fn poll(
        &self,
        ctx: &ActionCtx,
        trigger: &str,
        input: &Value,
        cursor: Option<&Value>,
    ) -> Result<PollResult, ConnectorError> {
        if trigger != "new_items" {
            return Err(ConnectorError::unsupported(format!(
                "polling for `{trigger}`"
            )));
        }
        let url = auth::url_for(&base_url(ctx)?, &req_str(input, "path")?)?;
        let resp = self
            .send(ctx, || {
                let query = key_values(input, "query");
                let refs: Vec<(&str, &str)> = query
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str()))
                    .collect();
                Ok(HttpRequest::new(Method::Get, url.clone())
                    .query(&refs)
                    .timeout(ctx.timeout))
            })
            .await?;
        if !resp.is_success() {
            return Err(ConnectorError::from_http(&resp));
        }
        let body = resp.json()?;
        let items = items_at(&body, &opt_str(input, "items_path").unwrap_or_default()).ok_or_else(
            || ConnectorError::invalid_field("items_path", "The answer has no list at this path"),
        )?;
        let id_field = opt_str(input, "id_field").unwrap_or_else(|| "id".into());
        let seen: Vec<String> = cursor
            .and_then(|c| c.get("seen"))
            .and_then(Value::as_array)
            .map(|ids| {
                ids.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        let ids: Vec<(String, &Value)> = items
            .iter()
            .filter_map(|i| item_id(i, &id_field).map(|id| (id, i)))
            .collect();
        // The first poll only remembers what is there: items created after the flow is switched on start runs.
        let new: Vec<EventItem> = if cursor.is_none() {
            Vec::new()
        } else {
            ids.iter()
                .filter(|(id, _)| !seen.contains(id))
                .map(|(id, item)| EventItem {
                    dedupe_key: format!("item:{id}"),
                    record_id: Some(id.clone()),
                    data: json!({"id": id, "item": item}),
                })
                .collect()
        };
        let mut remembered: Vec<String> = ids.into_iter().map(|(id, _)| id).collect();
        let older: Vec<String> = seen
            .into_iter()
            .filter(|s| !remembered.contains(s))
            .collect();
        remembered.extend(older);
        remembered.truncate(SEEN_MAX);
        Ok(PollResult {
            items: new,
            cursor: Some(json!({"seen": remembered})),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_stay_under_the_base_url() {
        let base = "https://api.example.com/v1/";
        assert_eq!(
            auth::url_for(base, "/orders/7").unwrap(),
            "https://api.example.com/v1/orders/7"
        );
        assert_eq!(
            auth::url_for(base, "orders").unwrap(),
            "https://api.example.com/v1/orders"
        );
        assert_eq!(
            auth::url_for(base, "").unwrap(),
            "https://api.example.com/v1"
        );
        for bad in [
            "https://evil.example/x",
            "//evil.example/x",
            "\\\\evil",
            "@evil.example/x",
        ] {
            let url = auth::url_for("https://api.example.com", bad);
            assert!(
                url.is_err()
                    || url
                        .as_deref()
                        .is_ok_and(|u| u.starts_with("https://api.example.com/")),
                "{bad}: {url:?}"
            );
        }
        assert!(auth::url_for("ftp://api.example.com", "/x").is_err());
        assert_eq!(
            items_at(&json!({"data": {"items": [1]}}), "data.items"),
            Some(&vec![json!(1)])
        );
        assert_eq!(
            items_at(&json!([1, 2]), ""),
            Some(&vec![json!(1), json!(2)])
        );
        assert_eq!(item_id(&json!({"id": 42}), "id").as_deref(), Some("42"));
    }
}
