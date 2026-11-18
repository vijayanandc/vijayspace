//! weft-connectors: built-in connectors. `builtin_connectors()` is the ONLY registration list.
//!
//! - `core.*`: utilities that need no connection (HTTP request, formatter, email).
//! - `http_api`: any app with a REST API (a connection with a base URL and credentials; like Zoho Flow's custom apps).
//! - `zoho_crm`, `appblocks`, `autodesk_forma`: app connectors (see `docs/plan/13-mvp-zoho-crm-appblocks.md`).
//!
//! `core.sync` lives in `weft-app` because it needs platform ports (record links, leases).

use std::sync::Arc;

use weft_connector_sdk::Connector;

pub mod appblocks;
pub mod autodesk_forma;
pub mod core;
pub mod http_api;
pub(crate) mod list_cursor;
mod util;
pub mod zoho_crm;

/// Operator settings some connectors take.
#[derive(Debug, Clone, Default)]
pub struct ConnectorOptions {
    /// Zoho accounts server to use instead of Zoho's (`WEFT_ZOHO_ACCOUNTS_URL`; testing against a mock).
    pub zoho_accounts_url: Option<String>,
}

/// Every built-in connector, in catalog order.
#[must_use]
pub fn builtin_connectors() -> Vec<Arc<dyn Connector>> {
    builtin_connectors_with(&ConnectorOptions::default())
}

/// Every built-in connector, with operator settings.
#[must_use]
pub fn builtin_connectors_with(options: &ConnectorOptions) -> Vec<Arc<dyn Connector>> {
    let zoho = options.zoho_accounts_url.as_deref().map_or_else(
        zoho_crm::ZohoCrmConnector::new,
        zoho_crm::ZohoCrmConnector::with_accounts_server,
    );
    vec![
        Arc::new(core::http::HttpConnector::new()),
        Arc::new(core::formatter::FormatterConnector::new()),
        Arc::new(core::email::EmailConnector::new()),
        Arc::new(core::script::ScriptConnector::new()),
        Arc::new(http_api::HttpApiConnector::new()),
        Arc::new(zoho),
        Arc::new(appblocks::AppblocksConnector::new()),
        Arc::new(autodesk_forma::FormaConnector::new()),
    ]
}
