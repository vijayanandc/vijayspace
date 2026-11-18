//! The platform mailer as flows use it (`core.email`): a recipient cap per message and a daily quota per org, so a
//! flow (anyone can create an org) cannot turn the platform's sender address into an open relay.

use std::sync::Arc;

use async_trait::async_trait;
use weft_connector_sdk::{ConnectorError, EmailMessage, Mailer};
use weft_domain::ids::OrgId;

use crate::Deps;

/// Recipients (to + cc) in one message.
pub const MAX_RECIPIENTS: usize = 20;
/// Messages an org's flows may send per day (UTC).
pub const FLOW_EMAILS_PER_DAY: u32 = 200;

/// Wraps the platform mailer for one org's flows.
pub struct QuotaMailer {
    pub(crate) deps: Deps,
    pub(crate) org: OrgId,
    pub(crate) inner: Arc<dyn Mailer>,
}

impl std::fmt::Debug for QuotaMailer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QuotaMailer")
            .field("org", &self.org)
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl Mailer for QuotaMailer {
    async fn send(&self, message: &EmailMessage) -> Result<(), ConnectorError> {
        if message.to.len() + message.cc.len() > MAX_RECIPIENTS {
            return Err(ConnectorError::invalid_field(
                "to",
                format!("Send to at most {MAX_RECIPIENTS} recipients (to and cc) per email"),
            ));
        }
        let now = self.deps.clock.now();
        let scope = format!("email:org:{}", self.org);
        let allowed = self
            .deps
            .store
            .take_send_quota(&[(scope, FLOW_EMAILS_PER_DAY)], now.date_naive(), now)
            .await
            .map_err(|e| ConnectorError::transient(e.message))?;
        if !allowed {
            return Err(ConnectorError::permanent(format!(
                "This organisation's flows have sent {FLOW_EMAILS_PER_DAY} emails today, the daily limit"
            )));
        }
        self.inner.send(message).await
    }
}
