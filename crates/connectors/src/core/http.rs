//! `core.http`: generic HTTP request (also serves as "outgoing webhook").

use std::time::Duration;

use weft_connector_sdk::prelude::*;

use crate::util::{key_values, opt_bool, opt_f64, opt_str, req_str};

/// The HTTP request connector.
#[derive(Debug)]
pub struct HttpConnector {
    manifest: ConnectorManifest,
}

impl HttpConnector {
    #[must_use]
    pub fn new() -> Self {
        let request = ActionSpec::new("request", "Send HTTP request", "Call any HTTP API or send an outgoing webhook.")
            .field(FieldSpec::select("method", "Method").options(&[("GET", "GET"), ("POST", "POST"), ("PUT", "PUT"), ("PATCH", "PATCH"), ("DELETE", "DELETE")]).default_value(json!("POST")))
            .field(FieldSpec::string("url", "URL").required().placeholder("https://api.example.com/items"))
            .field(FieldSpec::new("headers", "Headers", FieldKind::KeyValue))
            .field(FieldSpec::new("query", "Query parameters", FieldKind::KeyValue))
            .field(FieldSpec::select("body_type", "Body type").options(&[("json", "JSON"), ("form", "Form"), ("text", "Text"), ("none", "None")]).default_value(json!("json")))
            .field(FieldSpec::new("body", "Body", FieldKind::Json).help("JSON body; for form bodies use an object of fields"))
            .field(FieldSpec::new("fail_on_error_status", "Fail on non-2xx status", FieldKind::Boolean).default_value(json!(true)).advanced())
            .field(FieldSpec::new("send_idempotency_key", "Send Idempotency-Key header", FieldKind::Boolean).advanced())
            .field(FieldSpec::new("timeout_secs", "Timeout (seconds)", FieldKind::Number).default_value(json!(30)).advanced())
            .output(json!({"status": 200, "headers": {"content-type": "application/json"}, "body": {"id": "123"}}));
        Self {
            manifest: ConnectorManifest {
                key: "core.http".into(),
                name: "HTTP / Webhook".into(),
                description: "Send HTTP requests to any API.".into(),
                icon: "🌐".into(),
                category: "Utilities".into(),
                auth: AuthSpec::none(),
                actions: vec![request],
                triggers: Vec::new(),
                docs_url: None,
            },
        }
    }
}

impl Default for HttpConnector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Connector for HttpConnector {
    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    /// GET, HEAD, PUT and DELETE are idempotent by HTTP's own rules; POST and PATCH are not.
    fn is_idempotent(&self, _action: &str, input: &Value) -> bool {
        let method = input
            .get("method")
            .and_then(Value::as_str)
            // The same default `execute` sends.
            .unwrap_or("POST")
            .to_ascii_uppercase();
        matches!(method.as_str(), "GET" | "HEAD" | "PUT" | "DELETE")
    }

    async fn execute(
        &self,
        ctx: &ActionCtx,
        action: &str,
        input: Value,
    ) -> Result<Value, ConnectorError> {
        if action != "request" {
            return Err(ConnectorError::unsupported(action));
        }
        let method = Method::parse(&opt_str(&input, "method").unwrap_or_else(|| "POST".into()))?;
        let url = req_str(&input, "url")?;
        let query = key_values(&input, "query");
        let query_refs: Vec<(&str, &str)> = query
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let mut req = HttpRequest::new(method, url).query(&query_refs);
        let timeout = opt_f64(&input, "timeout_secs")
            .unwrap_or(30.0)
            .clamp(1.0, 120.0);
        req = req.timeout(Duration::from_secs_f64(timeout));
        for (k, v) in key_values(&input, "headers") {
            req = req.header(&k, v);
        }
        if opt_bool(&input, "send_idempotency_key").unwrap_or(false) {
            req = req.header("Idempotency-Key", ctx.idempotency_key.clone());
        }
        let body = input.get("body").cloned().unwrap_or(Value::Null);
        if method != Method::Get && !body.is_null() {
            req = match opt_str(&input, "body_type").as_deref().unwrap_or("json") {
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
            };
        }
        let resp = ctx.http.send(req).await?;
        if opt_bool(&input, "fail_on_error_status").unwrap_or(true) && !resp.is_success() {
            // No Weft connection is involved: a 401 means this request's own credentials are wrong (a step
            // setting to fix), not a connection to sign in to again.
            let err = ConnectorError::from_http(&resp);
            return Err(if let ConnectorError::AuthExpired { message } = err {
                ConnectorError::invalid(format!(
                    "{message} (the app refused this request's credentials: check its headers)"
                ))
            } else {
                err
            });
        }
        let headers: Map<String, Value> = resp
            .headers
            .iter()
            .map(|(k, v)| (k.to_lowercase(), Value::String(v.clone())))
            .collect();
        let parsed = resp.json().unwrap_or_else(|_| Value::String(resp.text()));
        Ok(json!({"status": resp.status, "headers": headers, "body": parsed}))
    }
}
