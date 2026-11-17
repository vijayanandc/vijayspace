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
        Self { kind: AuthKind::None, fields: Vec::new(), oauth2: None }
    }

    #[must_use]
    pub const fn fields(fields: Vec<FieldSpec>) -> Self {
        Self { kind: AuthKind::Fields, fields, oauth2: None }
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
        self.fields.iter().filter(|f| f.required).map(|f| f.key.clone()).collect()
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

/// Field widget types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    String,
    Text,
    Number,
    Boolean,
    Select,
    Datetime,
    Json,
    KeyValue,
    /// A group whose sub-fields are loaded dynamically (`fields_from`) — e.g. a record's fields.
    Object,
    Code,
    Password,
}

/// A dropdown option.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptionItem {
    pub value: String,
    pub label: String,
}

impl OptionItem {
    #[must_use]
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self { value: value.into(), label: label.into() }
    }
}

/// One input field. The UI renders forms from these; validation checks `required`.
#[allow(clippy::struct_excessive_bools)] // a UI schema: each flag is an independent rendering option
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldSpec {
    pub key: String,
    pub label: String,
    #[serde(rename = "type")]
    pub kind: FieldKind,
    #[serde(default)]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<OptionItem>,
    /// Dynamic dropdown: name of an option source (`Connector::options`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options_from: Option<String>,
    /// Dynamic sub-fields for `Object` fields (`Connector::fields`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fields_from: Option<String>,
    /// Re-load options/fields when these fields change.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
    /// Accepts `{{ … }}` variables (default true).
    #[serde(default = "yes")]
    pub mappable: bool,
    #[serde(default)]
    pub multiple: bool,
    #[serde(default)]
    pub advanced: bool,
    /// Auth fields only: stored encrypted.
    #[serde(default)]
    pub secret: bool,
    /// Read-only in the target app (shown as a source, never as a mapping target).
    #[serde(default)]
    pub read_only: bool,
}

const fn yes() -> bool {
    true
}

impl FieldSpec {
    #[must_use]
    pub fn new(key: &str, label: &str, kind: FieldKind) -> Self {
        Self {
            key: key.to_owned(),
            label: label.to_owned(),
            kind,
            required: false,
            help: None,
            placeholder: None,
            default: None,
            options: Vec::new(),
            options_from: None,
            fields_from: None,
            depends_on: Vec::new(),
            mappable: true,
            multiple: false,
            advanced: false,
            secret: false,
            read_only: false,
        }
    }

    #[must_use]
    pub fn string(key: &str, label: &str) -> Self {
        Self::new(key, label, FieldKind::String)
    }

    #[must_use]
    pub fn text(key: &str, label: &str) -> Self {
        Self::new(key, label, FieldKind::Text)
    }

    #[must_use]
    pub fn select(key: &str, label: &str) -> Self {
        Self::new(key, label, FieldKind::Select)
    }

    #[must_use]
    pub const fn required(mut self) -> Self {
        self.required = true;
        self
    }

    #[must_use]
    pub fn help(mut self, help: &str) -> Self {
        self.help = Some(help.to_owned());
        self
    }

    #[must_use]
    pub fn placeholder(mut self, placeholder: &str) -> Self {
        self.placeholder = Some(placeholder.to_owned());
        self
    }

    #[must_use]
    pub fn default_value(mut self, value: Value) -> Self {
        self.default = Some(value);
        self
    }

    #[must_use]
    pub fn options(mut self, options: &[(&str, &str)]) -> Self {
        self.options = options.iter().map(|(v, l)| OptionItem::new(*v, *l)).collect();
        self
    }

    #[must_use]
    pub fn options_from(mut self, source: &str) -> Self {
        self.options_from = Some(source.to_owned());
        self
    }

    #[must_use]
    pub fn fields_from(mut self, source: &str) -> Self {
        self.fields_from = Some(source.to_owned());
        self
    }

    #[must_use]
    pub fn depends_on(mut self, keys: &[&str]) -> Self {
        self.depends_on = keys.iter().map(|k| (*k).to_owned()).collect();
        self
    }

    #[must_use]
    pub const fn not_mappable(mut self) -> Self {
        self.mappable = false;
        self
    }

    #[must_use]
    pub const fn advanced(mut self) -> Self {
        self.advanced = true;
        self
    }

    #[must_use]
    pub const fn secret(mut self) -> Self {
        self.secret = true;
        self
    }

    #[must_use]
    pub const fn multiple(mut self) -> Self {
        self.multiple = true;
        self
    }

    #[must_use]
    pub const fn read_only(mut self) -> Self {
        self.read_only = true;
        self
    }
}
