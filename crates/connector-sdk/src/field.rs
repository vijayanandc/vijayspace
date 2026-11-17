//! Field specs: the UI schema of action, trigger and connection inputs (rendered by the client).

use serde::{Deserialize, Serialize};
use serde_json::Value;

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
        Self {
            value: value.into(),
            label: label.into(),
        }
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
        self.options = options
            .iter()
            .map(|(v, l)| OptionItem::new(*v, *l))
            .collect();
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
