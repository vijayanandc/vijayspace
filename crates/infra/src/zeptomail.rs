//! Email through the ZeptoMail API (as the Appblocks servers send it): `POST /v1.1/email` with
//! `Authorization: Zoho-enczapikey <key>`. The key comes only from the environment.

use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};
use weft_connector_sdk::{ConnectorError, EmailMessage, Mailer};

pub const DEFAULT_API_URL: &str = "https://api.zeptomail.com/v1.1/email";

/// Sends email with ZeptoMail.
pub struct ZeptoMailer {
    http: reqwest::Client,
    url: String,
    api_key: String,
    from_address: String,
    from_name: String,
}

impl std::fmt::Debug for ZeptoMailer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ZeptoMailer")
            .field("url", &self.url)
            .field("from", &self.from_address)
            .finish_non_exhaustive()
    }
}

impl ZeptoMailer {
    pub fn new(
        url: &str,
        api_key: &str,
        from_address: &str,
        from_name: &str,
    ) -> Result<Self, String> {
        if api_key.trim().is_empty() || !from_address.contains('@') {
            return Err("ZeptoMail needs an API key and a from address".into());
        }
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            http,
            url: url.to_owned(),
            api_key: api_key.trim().to_owned(),
            from_address: from_address.trim().to_owned(),
            from_name: from_name.trim().to_owned(),
        })
    }
}

fn recipients(addresses: &[String]) -> Vec<Value> {
    addresses
        .iter()
        .map(|a| json!({"email_address": {"address": a.trim()}}))
        .collect()
}

/// The request body for one message.
#[must_use]
pub fn payload(from_address: &str, from_name: &str, m: &EmailMessage) -> Value {
    let mut body = json!({
        "from": {"address": from_address, "name": from_name},
        "to": recipients(&m.to),
        "subject": m.subject,
    });
    body[if m.html { "htmlbody" } else { "textbody" }] = json!(m.body);
    if !m.cc.is_empty() {
        body["cc"] = json!(recipients(&m.cc));
    }
    if let Some(reply_to) = &m.reply_to {
        body["reply_to"] = json!([{"address": reply_to}]);
    }
    body
}

#[async_trait]
impl Mailer for ZeptoMailer {
    async fn send(&self, m: &EmailMessage) -> Result<(), ConnectorError> {
        if m.to.is_empty() {
            return Err(ConnectorError::invalid_field(
                "to",
                "Add at least one recipient",
            ));
        }
        let response = self
            .http
            .post(&self.url)
            .header("Accept", "application/json")
            .header("Authorization", format!("Zoho-enczapikey {}", self.api_key))
            .json(&payload(&self.from_address, &self.from_name, m))
            .send()
            .await
            .map_err(|e| ConnectorError::transient(format!("ZeptoMail: {e}")))?;
        let status = response.status();
        if status.is_success() {
            return Ok(());
        }
        let detail: String = response
            .text()
            .await
            .unwrap_or_default()
            .chars()
            .take(500)
            .collect();
        let message = format!("ZeptoMail returned {status}: {detail}");
        if status.is_server_error() || status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            Err(ConnectorError::transient(message))
        } else {
            Err(ConnectorError::permanent(message))
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_json, header, method},
    };

    use super::*;

    fn message(html: bool) -> EmailMessage {
        EmailMessage {
            to: vec!["ann@example.com".into()],
            cc: vec!["bo@example.com".into()],
            subject: "Hi".into(),
            body: "<p>Hello</p>".into(),
            html,
            reply_to: Some("ops@example.com".into()),
        }
    }

    #[tokio::test]
    async fn sends_the_zeptomail_request_with_the_api_key() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(header("authorization", "Zoho-enczapikey test-key"))
            .and(body_json(json!({
                "from": {"address": "noreply@example.com", "name": "Weft"},
                "to": [{"email_address": {"address": "ann@example.com"}}],
                "cc": [{"email_address": {"address": "bo@example.com"}}],
                "reply_to": [{"address": "ops@example.com"}],
                "subject": "Hi",
                "htmlbody": "<p>Hello</p>",
            })))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({"data": []})))
            .expect(1)
            .mount(&server)
            .await;
        let mailer =
            ZeptoMailer::new(&server.uri(), "test-key", "noreply@example.com", "Weft").unwrap();
        mailer.send(&message(true)).await.unwrap();
        assert_eq!(
            payload("a@b.c", "", &message(false))["textbody"],
            "<p>Hello</p>"
        );
    }

    #[tokio::test]
    async fn rejections_are_permanent_and_outages_transient() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(400).set_body_string("bad from"))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;
        let mailer = ZeptoMailer::new(&server.uri(), "k", "noreply@example.com", "").unwrap();
        let rejected = mailer.send(&message(true)).await.unwrap_err();
        assert!(
            !rejected.retryable() && rejected.to_string().contains("bad from"),
            "{rejected}"
        );
        assert!(mailer.send(&message(true)).await.unwrap_err().retryable());
        assert!(ZeptoMailer::new(&server.uri(), "", "noreply@example.com", "").is_err());
    }
}
