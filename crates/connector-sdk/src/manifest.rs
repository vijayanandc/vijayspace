//! Connector manifests: auth, actions, triggers and the field schema that drives the UI forms.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Everything the platform and the UI need to know about a connector.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConnectorManifest {
    /// Stable key: `core.http`, `zoho_crm`, `appblocks`, `autodesk_forma`.
    pub key: String,
    pub name: String,
    pub description: String,
    /// A short emoji or an icon URL.
    pub icon: String,
    pub category: String,
    pub auth: AuthSpec,
    pub actions: Vec<ActionSpec>,
    #[serde(default)]
    pub triggers: Vec<TriggerSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs_url: Option<String>,
}

impl ConnectorManifest {
    #[must_use]
    pub fn action(&self, key: &str) -> Option<&ActionSpec> {
        self.actions.iter().find(|a| a.key == key)
    }

    #[must_use]
    pub fn trigger(&self, key: &str) -> Option<&TriggerSpec> {
        self.triggers.iter().find(|t| t.key == key)
    }

    #[must_use]
    pub fn requires_connection(&self) -> bool {
        self.auth.kind != AuthKind::None
    }
}

/// How a connection authenticates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthKind {
    /// No connection needed (`core.*` utilities).
    None,
    /// Credentials entered in a form (API key, token, basic auth, base URL…).
    Fields,
    /// OAuth 2.0 authorization code flow.
    Oauth2,
}

/// Connection setup: form fields (secret ones are encrypted) and/or OAuth.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthSpec {
    pub kind: AuthKind,
    /// Form fields shown when creating a connection. `secret: true` fields are encrypted;
    /// the rest are stored as non-secret connection config (e.g. base URL, region).
    #[serde(default)]
    pub fields: Vec<FieldSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oauth2: Option<OAuth2Spec>,
}

impl AuthSpec {
    #[must_use]
    pub const fn none() -> Self {
        Self {
            kind: AuthKind::None,
            fields: Vec::new(),
            oauth2: None,
        }
    }

    #[must_use]
    pub const fn fields(fields: Vec<FieldSpec>) -> Self {
        Self {
            kind: AuthKind::Fields,
            fields,
            oauth2: None,
        }
    }
}

/// How the client credentials are sent to the token endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientAuth {
    /// `client_id` / `client_secret` in the form body.
    Body,
    /// HTTP Basic authorization header.
    Basic,
}

/// OAuth 2.0 settings. URLs may use `{{ config.<key> }}` placeholders filled from the
/// connection config (e.g. Zoho's per-data-centre accounts server).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OAuth2Spec {
    pub authorize_url: String,
    pub token_url: String,
    pub scopes: Vec<String>,
    #[serde(default = "space")]
    pub scope_separator: String,
    #[serde(default)]
    pub extra_authorize_params: Vec<(String, String)>,
    #[serde(default)]
    pub pkce: bool,
    pub client_auth: ClientAuth,
}

fn space() -> String {
    " ".to_owned()
}

/// One operation of a connector.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionSpec {
    pub key: String,
    pub name: String,
    pub description: String,
    pub fields: Vec<FieldSpec>,
    #[serde(default)]
    pub output_sample: Value,
    /// Safe to repeat (affects behaviour after a crash between the call and the checkpoint).
    #[serde(default)]
    pub idempotent: bool,
}

impl ActionSpec {
    #[must_use]
    pub fn new(key: &str, name: &str, description: &str) -> Self {
        Self {
            key: key.to_owned(),
            name: name.to_owned(),
            description: description.to_owned(),
            fields: Vec::new(),
            output_sample: Value::Null,
            idempotent: false,
        }
    }

    #[must_use]
    pub fn field(mut self, field: FieldSpec) -> Self {
        self.fields.push(field);
        self
    }

    #[must_use]
    pub fn output(mut self, sample: Value) -> Self {
        self.output_sample = sample;
        self
    }

    #[must_use]
    pub const fn idempotent(mut self) -> Self {
        self.idempotent = true;
        self
    }

    /// Keys of required fields (used by publish validation).
    #[must_use]
    pub fn required_fields(&self) -> Vec<String> {
        self.fields
            .iter()
            .filter(|f| f.required)
            .map(|f| f.key.clone())
            .collect()
    }
}

/// How a trigger receives events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerKind {
    /// The app pushes events to a Weft URL (subscribed automatically or set up by hand).
    Instant,
    /// Weft asks the app for changes on an interval.
    Polling,
}

/// One trigger of a connector.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TriggerSpec {
    pub key: String,
    pub name: String,
    pub description: String,
    pub kind: TriggerKind,
    pub fields: Vec<FieldSpec>,
    #[serde(default)]
    pub output_sample: Value,
    /// Instant triggers that the app cannot register automatically show setup instructions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup_instructions: Option<String>,
    /// Instant triggers that also support a polling reconciliation.
    #[serde(default)]
    pub supports_polling: bool,
}

impl TriggerSpec {
    #[must_use]
    pub fn new(key: &str, name: &str, description: &str, kind: TriggerKind) -> Self {
        Self {
            key: key.to_owned(),
            name: name.to_owned(),
            description: description.to_owned(),
            kind,
            fields: Vec::new(),
            output_sample: Value::Null,
            setup_instructions: None,
            supports_polling: kind == TriggerKind::Polling,
        }
    }

    #[must_use]
    pub fn field(mut self, field: FieldSpec) -> Self {
        self.fields.push(field);
        self
    }

    #[must_use]
    pub fn output(mut self, sample: Value) -> Self {
        self.output_sample = sample;
        self
    }

    #[must_use]
    pub fn setup(mut self, instructions: &str) -> Self {
        self.setup_instructions = Some(instructions.to_owned());
        self
    }

    #[must_use]
    pub const fn with_polling(mut self) -> Self {
        self.supports_polling = true;
        self
    }
}

pub use crate::field::{FieldKind, FieldSpec, OptionItem};
