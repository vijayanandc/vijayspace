//! weft-connector-sdk: the contract every connector implements.
//!
//! A connector = manifest (auth, actions, triggers, fields) + `Connector` trait implementation.
//! Connectors talk to the outside world only through `ActionCtx` (HTTP sender, connection handle,
//! mailer), which keeps them testable against mock servers. See `docs/plan/07-connectors.md`.

mod connector;
mod context;
mod error;
mod field;
mod http;
mod manifest;
pub mod oauth;
mod records;
mod registry;
#[cfg(feature = "testing")]
pub mod testing;

pub use connector::{
    Connector, EventBatch, EventItem, InboundRequest, PollResult, SubscribeRequest,
    SubscriptionInfo,
};
pub use context::{ActionCtx, ConnectionHandle, EmailMessage, Mailer, ScriptRun, ScriptRunner};
pub use error::ConnectorError;
pub use http::{HttpRequest, HttpResponse, HttpSender, Method};
pub use manifest::{
    ActionSpec, AuthKind, AuthSpec, ClientAuth, ConnectorManifest, FieldKind, FieldSpec,
    OAuth2Spec, OptionItem, TriggerKind, TriggerSpec,
};
pub use records::{ObjectInfo, Page, Record, RecordObjects, RecordRef};
pub use registry::ConnectorRegistry;

/// Common imports for connector implementations.
pub mod prelude {
    pub use async_trait::async_trait;
    pub use serde_json::{Map, Value, json};

    pub use crate::{
        ActionCtx, ActionSpec, AuthSpec, Connector, ConnectorError, ConnectorManifest, FieldKind,
        FieldSpec, HttpRequest, HttpResponse, Method, OptionItem, TriggerKind, TriggerSpec,
    };
}
