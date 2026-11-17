//! weft-connectors: built-in connectors. `builtin_connectors()` is the ONLY registration list.
//!
//! - `core.*`: utilities that need no connection (HTTP request, formatter, email).
//! - `zoho_crm`, `appblocks`, `autodesk_forma`: app connectors (see `docs/plan/13-mvp-zoho-crm-appblocks.md`).
//!
//! `core.sync` lives in `weft-app` because it needs platform ports (record links, leases).

use std::sync::Arc;

use weft_connector_sdk::Connector;

pub mod appblocks;
pub mod autodesk_forma;
pub mod core;
pub(crate) mod list_cursor;
mod util;
pub mod zoho_crm;

/// Every built-in connector, in catalog order.
#[must_use]
pub fn builtin_connectors() -> Vec<Arc<dyn Connector>> {
    vec![
        Arc::new(core::http::HttpConnector::new()),
        Arc::new(core::formatter::FormatterConnector::new()),
        Arc::new(core::email::EmailConnector::new()),
        Arc::new(zoho_crm::ZohoCrmConnector::new()),
        Arc::new(appblocks::AppblocksConnector::new()),
        Arc::new(autodesk_forma::FormaConnector::new()),
    ]
}
