//! Email delivery: SMTP (lettre) or a logging fallback for development.

use async_trait::async_trait;
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::{Mailbox, header::ContentType},
};
use weft_connector_sdk::{ConnectorError, EmailMessage, Mailer};

/// SMTP mailer (`smtps://user:pass@host:465` or `smtp://host:1025` for Mailpit).
pub struct SmtpMailer {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
}

impl std::fmt::Debug for SmtpMailer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SmtpMailer")
            .field("from", &self.from.to_string())
            .finish_non_exhaustive()
    }
}

impl SmtpMailer {
    pub fn new(url: &str, from: &str) -> Result<Self, String> {
        let transport = AsyncSmtpTransport::<Tokio1Executor>::from_url(url)
            .map_err(|e| e.to_string())?
            .build();
        let from = from
            .parse()
            .map_err(|e| format!("invalid from address: {e}"))?;
        Ok(Self { transport, from })
    }
}

fn mailbox(addr: &str) -> Result<Mailbox, ConnectorError> {
    addr.parse()
        .map_err(|_| ConnectorError::invalid_field("to", format!("invalid address `{addr}`")))
}

#[async_trait]
impl Mailer for SmtpMailer {
    async fn send(&self, m: &EmailMessage) -> Result<(), ConnectorError> {
        let mut builder = Message::builder()
            .from(self.from.clone())
            .subject(m.subject.clone());
        for to in &m.to {
            builder = builder.to(mailbox(to)?);
        }
        for cc in &m.cc {
            builder = builder.cc(mailbox(cc)?);
        }
        if let Some(r) = &m.reply_to {
            builder = builder.reply_to(mailbox(r)?);
        }
        let content_type = if m.html {
            ContentType::TEXT_HTML
        } else {
            ContentType::TEXT_PLAIN
        };
        let message = builder
            .header(content_type)
            .body(m.body.clone())
            .map_err(|e| ConnectorError::invalid(e.to_string()))?;
        self.transport
            .send(message)
            .await
            .map(|_| ())
            .map_err(|e| ConnectorError::transient(format!("SMTP: {e}")))
    }
}

/// Logs emails instead of sending them (when no SMTP server is configured).
#[derive(Debug, Default)]
pub struct LogMailer;

#[async_trait]
impl Mailer for LogMailer {
    async fn send(&self, m: &EmailMessage) -> Result<(), ConnectorError> {
        tracing::info!(to = ?m.to, subject = %m.subject, "email (not sent: SMTP is not configured)");
        Ok(())
    }
}
