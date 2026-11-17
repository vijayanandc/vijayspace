//! Thin APS HTTP client: base URL (overridable for tests), bearer auth, one refresh on 401.

use weft_connector_sdk::prelude::*;

/// Production APS host.
pub(crate) const DEFAULT_BASE: &str = "https://developer.api.autodesk.com";

/// A request helper bound to one action call.
#[derive(Debug)]
pub(crate) struct Forma<'a> {
    ctx: &'a ActionCtx,
    base: String,
    region: String,
}

impl<'a> Forma<'a> {
    pub(crate) fn new(ctx: &'a ActionCtx) -> Result<Self, ConnectorError> {
        let config = ctx.conn()?.config();
        let text = |key: &str| {
            config
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
        };
        Ok(Self {
            ctx,
            base: text("base_url")
                .unwrap_or(DEFAULT_BASE)
                .trim_end_matches('/')
                .to_owned(),
            region: text("region").unwrap_or("US").to_owned(),
        })
    }

    pub(crate) fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    /// The data region (`US`, `EMEA`, `AUS`) sent as `x-ads-region` on webhook calls.
    pub(crate) fn region(&self) -> &str {
        &self.region
    }

    /// Sends with the connection's bearer token; on 401 refreshes once and retries.
    pub(crate) async fn send(&self, req: HttpRequest) -> Result<HttpResponse, ConnectorError> {
        let conn = self.ctx.conn()?;
        let token = conn.access_token().await?;
        let resp = self
            .ctx
            .http
            .send(req.clone().bearer(&token).timeout(self.ctx.timeout))
            .await?;
        if resp.status != 401 {
            return Ok(resp);
        }
        let token = conn.force_refresh().await?;
        self.ctx
            .http
            .send(req.bearer(&token).timeout(self.ctx.timeout))
            .await
    }

    /// Sends and returns the JSON body of a successful response.
    pub(crate) async fn json(&self, req: HttpRequest) -> Result<Value, ConnectorError> {
        let resp = self.send(req).await?;
        check(&resp)?;
        resp.json()
    }
}

/// Maps non-2xx responses; 403 gets a message that names the likely cause.
pub(crate) fn check(resp: &HttpResponse) -> Result<(), ConnectorError> {
    if resp.is_success() {
        return Ok(());
    }
    if resp.status == 403 {
        let detail: String = resp.text().chars().take(300).collect();
        return Err(ConnectorError::Permanent {
            status: Some(403),
            message: format!(
                "autodesk.permission: the connected Forma user lacks permission for this account or project \
                 (account admin or project admin may be required): {detail}"
            ),
        });
    }
    Err(ConnectorError::from_http(resp))
}

/// URL-encodes one path segment.
pub(crate) fn seg(s: &str) -> String {
    urlencoding::encode(s).into_owned()
}

/// Construction APIs use IDs without the Data Management `b.` prefix.
pub(crate) fn plain_id(id: &str) -> &str {
    id.strip_prefix("b.").unwrap_or(id)
}

/// The list in a paged APS response (`results` for construction APIs, `data` for Data Management).
pub(crate) fn results(body: &Value) -> Vec<Value> {
    body.get("results")
        .or_else(|| body.get("data"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// The Forma account: `account_id` from the step input, else from the connection config.
pub(crate) fn account_id(ctx: &ActionCtx, input: &Value) -> Result<String, ConnectorError> {
    crate::util::opt_str(input, "account_id")
        .or_else(|| ctx.config_str("account_id").ok())
        .map(|a| plain_id(&a).to_owned())
        .ok_or_else(|| {
            ConnectorError::invalid_field(
                "account_id",
                "Choose a Forma account (or set it on the connection)",
            )
        })
}
