//! `http_api` ("Any app (HTTP API)"): connect any app with a REST API, like Zoho Flow's custom apps.
//!
//! - **Connection:** a base URL and how requests are signed; credentials are encrypted in the connection, never in
//!   a flow (see `auth`).
//! - **Action:** "Make an API call" to a path under the base URL.
//! - **Triggers:** "New items" polls a list endpoint and starts a run per item not seen before; "New or changed
//!   items" also starts one when an item changes (see `poll`). Apps that push events use the Webhook trigger.

mod auth;
mod manifest;
mod poll;

use std::time::Duration;

use weft_connector_sdk::{OAuth2Spec, PollResult, prelude::*};

use crate::util::{key_values, opt_bool, opt_f64, opt_str, req_str};

/// The "Any app" connector.
#[derive(Debug)]
pub struct HttpApiConnector {
    manifest: ConnectorManifest,
    tokens: auth::TokenCache,
    /// "Any app (OAuth 2.0 sign-in)": connections are OAuth sign-ins with the app's own client.
    sign_in: bool,
}

impl HttpApiConnector {
    #[must_use]
    pub fn new() -> Self {
        Self {
            manifest: manifest::manifest(),
            tokens: auth::TokenCache::default(),
            sign_in: false,
        }
    }

    /// "Any app (OAuth 2.0 sign-in)" (`http_oauth`): the same action and trigger, signed with an OAuth 2.0
    /// sign-in at the app through the app's own OAuth client.
    #[must_use]
    pub fn oauth() -> Self {
        Self {
            manifest: manifest::oauth_manifest(),
            tokens: auth::TokenCache::default(),
            sign_in: true,
        }
    }

    /// Sends a signed request. After a 401, an OAuth sign-in is refreshed and a client-credentials token fetched
    /// again, and the request is retried once.
    async fn send(
        &self,
        ctx: &ActionCtx,
        build: impl Fn() -> Result<HttpRequest, ConnectorError> + Send + Sync,
    ) -> Result<HttpResponse, ConnectorError> {
        let signed = auth::sign(ctx, &self.tokens, build()?, self.sign_in, None).await?;
        let resp = ctx.http.send(signed).await?;
        if resp.status != 401 {
            return Ok(resp);
        }
        let conn = ctx.conn()?;
        if self.sign_in {
            let fresh = conn.force_refresh().await?;
            let signed = auth::sign(ctx, &self.tokens, build()?, true, Some(&fresh)).await?;
            return ctx.http.send(signed).await;
        }
        if opt_str(conn.config(), "auth").as_deref() == Some("oauth2_client_credentials") {
            self.tokens.forget(conn.id());
            let signed = auth::sign(ctx, &self.tokens, build()?, false, None).await?;
            return ctx.http.send(signed).await;
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
        let headers = key_values(input, "headers");
        for (k, v) in &headers {
            auth::check_header(k, v)?;
        }
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
                for (k, v) in &headers {
                    req = req.header(k, v.clone());
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

#[async_trait]
impl Connector for HttpApiConnector {
    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    fn own_oauth_client(&self) -> bool {
        self.sign_in
    }

    /// The sign-in settings are the connection's own (the app's authorize and token URLs, scopes).
    fn oauth_spec(&self, config: &Value) -> Result<Option<OAuth2Spec>, ConnectorError> {
        if !self.sign_in {
            return Ok(None);
        }
        auth::sign_in_spec(config).map(Some)
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
        match trigger {
            "new_items" => poll::new_items(self, ctx, input, cursor).await,
            "changed_items" => poll::changed_items(self, ctx, input, cursor).await,
            other => Err(ConnectorError::unsupported(format!(
                "polling for `{other}`"
            ))),
        }
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
            poll::items_at(&json!({"data": {"items": [1]}}), "data.items"),
            Some(&vec![json!(1)])
        );
        assert_eq!(
            poll::items_at(&json!([1, 2]), ""),
            Some(&vec![json!(1), json!(2)])
        );
        assert_eq!(
            poll::item_id(&json!({"id": 42}), "id").as_deref(),
            Some("42")
        );
    }
}
