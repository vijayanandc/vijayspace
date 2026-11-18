//! `zoho_crm` connector: Zoho CRM API v8 (OAuth, all data centres, notifications).
//! See `docs/plan/13-mvp-zoho-crm-appblocks.md` §5.2.

mod client;
mod fields;
mod manifest;
mod records;
mod triggers;

use std::collections::BTreeMap;

use weft_connector_sdk::{
    EventBatch, InboundRequest, OAuth2Spec, PollResult, RecordObjects, SubscribeRequest,
    SubscriptionInfo, prelude::*,
};

use crate::util::{list, object, opt_bool, opt_f64, opt_str, req_str};

/// Zoho accounts servers (one per data centre). The first is the default (US).
pub(crate) const ACCOUNTS_SERVERS: [&str; 9] = [
    "https://accounts.zoho.com",
    "https://accounts.zoho.eu",
    "https://accounts.zoho.in",
    "https://accounts.zoho.com.au",
    "https://accounts.zoho.jp",
    "https://accounts.zoho.com.cn",
    "https://accounts.zohocloud.ca",
    "https://accounts.zoho.sa",
    "https://accounts.zoho.uk",
];

/// The Zoho CRM connector.
#[derive(Debug)]
pub struct ZohoCrmConnector {
    manifest: ConnectorManifest,
    /// The only accounts server used, instead of Zoho's data centres (testing against a mock).
    accounts_override: Option<String>,
}

impl ZohoCrmConnector {
    #[must_use]
    pub fn new() -> Self {
        Self {
            manifest: manifest::manifest(None),
            accounts_override: None,
        }
    }

    /// Signs in and gets tokens at `accounts` only (an operator setting, for testing against a Zoho mock):
    /// the authorize page is `{accounts}/oauth/v2/auth` and no other accounts server is accepted.
    #[must_use]
    pub fn with_accounts_server(accounts: &str) -> Self {
        let accounts = accounts.trim_end_matches('/').to_owned();
        Self {
            manifest: manifest::manifest(Some(&accounts)),
            accounts_override: Some(accounts),
        }
    }
}

impl Default for ZohoCrmConnector {
    fn default() -> Self {
        Self::new()
    }
}

fn values(input: &Value) -> Result<Map<String, Value>, ConnectorError> {
    let map = object(input, "values");
    if map.is_empty() {
        Err(ConnectorError::invalid_field(
            "values",
            "Map at least one field",
        ))
    } else {
        Ok(map)
    }
}

fn trigger_workflows(input: &Value) -> bool {
    opt_bool(input, "trigger_workflows").unwrap_or(false)
}

fn written(item: &Value) -> Result<Value, ConnectorError> {
    Ok(json!({"id": records::result_id(item)?, "record": item.get("details")}))
}

async fn search_action(ctx: &ActionCtx, input: &Value) -> Result<Value, ConnectorError> {
    let module = req_str(input, "module")?;
    let criteria = if let Some(c) = opt_str(input, "criteria") {
        c
    } else {
        let field = req_str(input, "field").map_err(|_| {
            ConnectorError::invalid_field("criteria", "Enter criteria, or a field and a value")
        })?;
        records::criteria_eq(&field, input.get("value").unwrap_or(&Value::Null))?
    };
    // Clamped to the API page size; the cast cannot truncate.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let limit = opt_f64(input, "limit")
        .unwrap_or(10.0)
        .clamp(1.0, f64::from(records::PAGE_LIMIT)) as u32;
    let rows = records::search(ctx, &module, &criteria, limit).await?;
    Ok(json!({"count": rows.len(), "first": rows.first(), "records": rows}))
}

async fn create_task(ctx: &ActionCtx, input: &Value) -> Result<Value, ConnectorError> {
    let mut task = Map::new();
    task.insert("Subject".into(), json!(req_str(input, "subject")?));
    if let Some(due) = opt_str(input, "due_date") {
        task.insert(
            "Due_Date".into(),
            json!(due.chars().take(10).collect::<String>()),
        );
    }
    if let Some(desc) = opt_str(input, "description") {
        task.insert("Description".into(), json!(desc));
    }
    if let (Some(module), Some(id)) = (opt_str(input, "module"), opt_str(input, "record_id")) {
        task.insert(
            "What_Id".into(),
            json!({"id": client::check_id("record_id", &id)?}),
        );
        task.insert("$se_module".into(), json!(client::check_module(&module)?));
    }
    written(&records::create_task(ctx, &task).await?)
}

#[async_trait]
impl Connector for ZohoCrmConnector {
    /// VERIFY per edition: Zoho grants API credits per day and concurrency by edition; these are conservative.
    fn default_limits(&self) -> ConnectionLimits {
        ConnectionLimits {
            requests_per_minute: Some(100),
            max_concurrent: Some(10),
            daily_requests: Some(25_000),
        }
    }

    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    async fn execute(
        &self,
        ctx: &ActionCtx,
        action: &str,
        input: Value,
    ) -> Result<Value, ConnectorError> {
        match action {
            "create_record" => written(
                &records::create(
                    ctx,
                    &req_str(&input, "module")?,
                    &values(&input)?,
                    trigger_workflows(&input),
                )
                .await?,
            ),
            "update_record" => written(
                &records::update(
                    ctx,
                    &req_str(&input, "module")?,
                    &req_str(&input, "record_id")?,
                    &values(&input)?,
                    trigger_workflows(&input),
                )
                .await?,
            ),
            "upsert_record" => {
                let item = records::upsert(
                    ctx,
                    &req_str(&input, "module")?,
                    &values(&input)?,
                    &list(&input, "duplicate_check_fields"),
                    trigger_workflows(&input),
                )
                .await?;
                Ok(
                    json!({"id": records::result_id(&item)?, "action": item.get("action"), "duplicate_field": item.get("duplicate_field"), "record": item.get("details")}),
                )
            }
            "get_record" => {
                let id = req_str(&input, "record_id")?;
                let rec = records::get(ctx, &req_str(&input, "module")?, &id)
                    .await?
                    .ok_or_else(|| ConnectorError::NotFound {
                        message: format!("record {id} not found"),
                    })?;
                Ok(json!({"id": id, "record": rec}))
            }
            "search_records" => search_action(ctx, &input).await,
            "add_note" => {
                let title = opt_str(&input, "title").unwrap_or_default();
                written(
                    &records::add_note(
                        ctx,
                        &req_str(&input, "module")?,
                        &req_str(&input, "record_id")?,
                        &title,
                        &req_str(&input, "content")?,
                    )
                    .await?,
                )
            }
            "create_task" => create_task(ctx, &input).await,
            other => Err(ConnectorError::unsupported(other)),
        }
    }

    async fn test_connection(&self, ctx: &ActionCtx) -> Result<Value, ConnectorError> {
        let url = format!("{}/org", client::base(ctx)?);
        let body = client::send(ctx, |t| client::request(Method::Get, &url, t)).await?;
        let org = body
            .get("org")
            .and_then(Value::as_array)
            .and_then(|o| o.first())
            .cloned()
            .unwrap_or(Value::Null);
        Ok(
            json!({"org_name": org.get("company_name"), "org_id": org.get("id"), "domain": org.get("domain_name")}),
        )
    }

    async fn options(
        &self,
        ctx: &ActionCtx,
        source: &str,
        input: &Value,
    ) -> Result<Vec<OptionItem>, ConnectorError> {
        match source {
            "modules" => Ok(fields::module_options(&records::modules(ctx).await?)),
            "module_field_names" => {
                let specs =
                    fields::field_specs(&records::fields(ctx, &req_str(input, "module")?).await?);
                Ok(specs
                    .into_iter()
                    .map(|f| OptionItem::new(f.key, f.label))
                    .collect())
            }
            other => Err(ConnectorError::unsupported(format!("options `{other}`"))),
        }
    }

    async fn fields(
        &self,
        ctx: &ActionCtx,
        source: &str,
        input: &Value,
    ) -> Result<Vec<FieldSpec>, ConnectorError> {
        match source {
            "module_fields" => Ok(fields::field_specs(
                &records::fields(ctx, &req_str(input, "module")?).await?,
            )),
            other => Err(ConnectorError::unsupported(format!("fields `{other}`"))),
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
            "record_event" => triggers::poll(ctx, input, cursor).await,
            other => Err(ConnectorError::unsupported(format!(
                "polling for `{other}`"
            ))),
        }
    }

    async fn subscribe(
        &self,
        ctx: &ActionCtx,
        trigger: &str,
        input: &Value,
        req: &SubscribeRequest,
    ) -> Result<SubscriptionInfo, ConnectorError> {
        match trigger {
            "record_event" => triggers::subscribe(ctx, input, req).await,
            other => Err(ConnectorError::unsupported(format!(
                "subscriptions for `{other}`"
            ))),
        }
    }

    async fn renew(
        &self,
        ctx: &ActionCtx,
        trigger: &str,
        input: &Value,
        _sub: &SubscriptionInfo,
        req: &SubscribeRequest,
    ) -> Result<SubscriptionInfo, ConnectorError> {
        match trigger {
            "record_event" => triggers::renew(ctx, input, req).await,
            other => Err(ConnectorError::unsupported(format!(
                "subscriptions for `{other}`"
            ))),
        }
    }

    async fn unsubscribe(
        &self,
        ctx: &ActionCtx,
        _trigger: &str,
        sub: &SubscriptionInfo,
    ) -> Result<(), ConnectorError> {
        triggers::unsubscribe(ctx, sub).await
    }

    async fn handle_event(
        &self,
        ctx: &ActionCtx,
        trigger: &str,
        input: &Value,
        request: &InboundRequest,
        verify_token: &str,
    ) -> Result<EventBatch, ConnectorError> {
        match trigger {
            "record_event" => triggers::handle_event(ctx, input, request, verify_token).await,
            other => Err(ConnectorError::unsupported(format!("events for `{other}`"))),
        }
    }

    fn records(&self) -> Option<&dyn RecordObjects> {
        Some(self)
    }

    /// Checked again at every token request (the stored accounts server may have been edited): the request
    /// carries the platform's client secret, so it only goes to Zoho's own accounts servers or the override.
    fn oauth_spec(&self, config: &Value) -> Result<Option<OAuth2Spec>, ConnectorError> {
        if let Some(accounts) = config.get("accounts_server").and_then(Value::as_str) {
            let trusted = self.accounts_override.as_deref().map_or_else(
                || ACCOUNTS_SERVERS.contains(&accounts),
                |only| only == accounts,
            );
            if !trusted {
                return Err(ConnectorError::invalid_field(
                    "accounts_server",
                    "Not a Zoho accounts server: sign in to Zoho again",
                ));
            }
        }
        Ok(self.manifest.auth.oauth2.clone())
    }

    fn oauth_callback_config(&self, query: &BTreeMap<String, String>) -> Value {
        // The token request carries the platform's client secret, so the accounts server must
        // be one of Zoho's own data centres (or the operator's override), never whatever the callback URL says.
        if let Some(only) = &self.accounts_override {
            return json!({"accounts_server": only, "location": query.get("location")});
        }
        let accounts = query
            .get("accounts-server")
            .map(|s| s.trim_end_matches('/'))
            .filter(|s| ACCOUNTS_SERVERS.contains(s))
            .unwrap_or(ACCOUNTS_SERVERS[0]);
        json!({"accounts_server": accounts, "location": query.get("location")})
    }

    fn oauth_token_config(&self, token_response: &Value) -> Value {
        match token_response.get("api_domain").and_then(Value::as_str) {
            Some(domain) => json!({"api_domain": domain}),
            None => Value::Null,
        }
    }
}
