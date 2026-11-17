//! The sync layer (`core.sync`): record links between two record apps, upsert-linked-record,
//! lookups through links, dynamic pickers, and backfill. See doc 13 §4.

mod backfill;
mod connector;
mod link;
mod pickers;
mod upsert;

pub use backfill::backfill;
pub use connector::SyncConnector;
pub use pickers::{fields, options};

use serde_json::Value;
use weft_connector_sdk::ConnectorError;
use weft_domain::ids::OrgId;

use crate::Deps;

/// Connector key of the sync utilities.
pub const CONNECTOR_KEY: &str = "core.sync";

/// Runs a `core.sync` action (called by the runner; needs platform ports, so it is not a plain connector).
pub async fn execute(
    deps: &Deps,
    org: OrgId,
    action: &str,
    input: &Value,
) -> Result<Value, ConnectorError> {
    match action {
        "upsert_linked_record" => upsert::upsert_linked_record(deps, org, input).await,
        "resolve_link" => link::resolve_link(deps, org, input, false).await,
        "find_linked_record" => link::resolve_link(deps, org, input, true).await,
        "save_link" => link::save_link(deps, org, input).await,
        other => Err(ConnectorError::unsupported(other)),
    }
}
