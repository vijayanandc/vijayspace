//! Ports: per-connection request limits (overrides) and daily usage counters.

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use weft_connector_sdk::ConnectionLimits;
use weft_domain::ids::{ConnectionId, OrgId};

use super::Res;

/// Requests made (and refused) through one connection on one UTC day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectionUsage {
    pub connection_id: ConnectionId,
    pub requests: u32,
    pub refused: u32,
}

#[async_trait]
pub trait LimitRepo: Send + Sync {
    /// The connection's overrides of the connector defaults (all `None` when unset).
    async fn get_connection_limits(&self, org: OrgId, id: ConnectionId) -> Res<ConnectionLimits>;
    async fn set_connection_limits(
        &self,
        org: OrgId,
        id: ConnectionId,
        overrides: &ConnectionLimits,
        now: DateTime<Utc>,
    ) -> Res<()>;
    /// Counts one request on `day` unless `cap` requests were already counted (atomic, so
    /// concurrent workers never exceed the cap); a refusal is counted instead. Returns whether the
    /// request may go ahead.
    async fn count_connection_request(
        &self,
        org: OrgId,
        id: ConnectionId,
        day: NaiveDate,
        cap: Option<u32>,
        now: DateTime<Utc>,
    ) -> Res<bool>;
    /// Today's counters for every connection of the org that made requests.
    async fn connection_usage(&self, org: OrgId, day: NaiveDate) -> Res<Vec<ConnectionUsage>>;
}
