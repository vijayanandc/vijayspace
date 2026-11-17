//! `core.email`: send email through the platform mailer (no connection needed).

use weft_connector_sdk::{EmailMessage, prelude::*};

use crate::util::{list, opt_bool, opt_str, req_str};

/// The email connector.
#[derive(Debug)]
pub struct EmailConnector {
    manifest: ConnectorManifest,
}

impl EmailConnector {
    #[must_use]
    pub fn new() -> Self {
        let send = ActionSpec::new("send", "Send email", "Send an email from the platform address.")
            .field(FieldSpec::string("to", "To").required().help("Comma-separated addresses"))
            .field(FieldSpec::string("cc", "Cc").advanced())
            .field(FieldSpec::string("subject", "Subject").required())
            .field(FieldSpec::text("body", "Body").required())
            .field(FieldSpec::new("html", "Body is HTML", FieldKind::Boolean).advanced())
            .field(FieldSpec::string("reply_to", "Reply-To").advanced())
            .output(json!({"sent": true, "to": ["customer@example.com"]}));
        Self {
            manifest: ConnectorManifest {
                key: "core.email".into(),
                name: "Email".into(),
                description: "Send email notifications.".into(),
                icon: "✉️".into(),
                category: "Utilities".into(),
                auth: AuthSpec::none(),
                actions: vec![send],
                triggers: Vec::new(),
                docs_url: None,
            },
        }
    }
}

impl Default for EmailConnector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Connector for EmailConnector {
    fn manifest(&self) -> &ConnectorManifest {
        &self.manifest
    }

    async fn execute(&self, ctx: &ActionCtx, action: &str, input: Value) -> Result<Value, ConnectorError> {
        if action != "send" {
            return Err(ConnectorError::unsupported(action));
        }
        let to = list(&input, "to");
        if to.is_empty() || to.iter().any(|a| !a.contains('@')) {
            return Err(ConnectorError::invalid_field("to", "Enter at least one valid email address"));
        }
        let message = EmailMessage {
            to: to.clone(),
            cc: list(&input, "cc"),
            subject: req_str(&input, "subject")?,
            body: req_str(&input, "body")?,
            html: opt_bool(&input, "html").unwrap_or(false),
            reply_to: opt_str(&input, "reply_to"),
        };
        let mailer = ctx.mailer.as_ref().ok_or_else(|| ConnectorError::permanent("Email sending is not configured on this server"))?;
        mailer.send(&message).await?;
        Ok(json!({"sent": true, "to": to}))
    }
}
