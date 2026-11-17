//! Leases on target records found through a match field, so two source records matching the
//! same target cannot both write it before one of them is linked.

use chrono::Duration;
use sha2::{Digest, Sha256};
use weft_connector_sdk::ConnectorError;
use weft_domain::{error_codes, ids::OrgId};

use super::upsert::LEASE_SECS;
use crate::{Deps, ports::RecordSide};

/// The target side of a sync step.
#[derive(Clone, Copy)]
pub(super) struct TargetRef<'a> {
    pub org: OrgId,
    pub set: &'a str,
    pub conn: weft_domain::ids::ConnectionId,
    pub object: &'a str,
}

/// Leases a record found through the match field (remembered in `claimed` so it is always
/// released) and tells whether it is already linked to another source record.
pub(super) async fn claim_target(
    deps: &Deps,
    t: TargetRef<'_>,
    source: &RecordSide,
    holder: &str,
    claimed: &std::sync::Mutex<Option<Vec<u8>>>,
    candidate: String,
) -> Result<bool, ConnectorError> {
    let key = target_lease_key(t.set, t.conn, t.object, &candidate);
    let now = deps.clock.now();
    let until = now + Duration::seconds(LEASE_SECS);
    let leased = deps
        .store
        .acquire_lease(t.org, &key, holder, until, now)
        .await
        .map_err(|e| ConnectorError::transient(e.to_string()))?;
    if !leased {
        return Err(busy());
    }
    if let Ok(mut slot) = claimed.lock() {
        *slot = Some(key);
    }
    let side = RecordSide {
        connection_id: t.conn,
        object: t.object.to_owned(),
        record_id: candidate,
    };
    let owner = deps
        .store
        .find_link_by_b(t.org, t.set, &side)
        .await
        .map_err(|e| ConnectorError::transient(e.to_string()))?;
    Ok(owner.is_some_and(|l| l.a != *source))
}

/// Lease key of a target record (distinct from source-record keys).
pub(super) fn target_lease_key(
    set: &str,
    conn: weft_domain::ids::ConnectionId,
    object: &str,
    id: &str,
) -> Vec<u8> {
    Sha256::digest(format!("{set}|target|{conn}|{object}|{id}").as_bytes()).to_vec()
}

/// Another sync of the same record is running: retried shortly.
pub(super) fn busy() -> ConnectorError {
    ConnectorError::RateLimited {
        retry_after_secs: Some(10),
        message: format!(
            "{}: another sync of this record is running",
            error_codes::SYNC_BUSY
        ),
    }
}
