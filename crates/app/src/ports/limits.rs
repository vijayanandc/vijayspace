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

/// Whether a request was counted, or which limit refused it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Counted {
    Allowed,
    /// The current minute already has `per_minute` requests.
    MinuteFull,
    /// The day already has `daily_cap` requests.
    DayFull,
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
    /// Counts one request on `now`'s day and minute unless the day already has `daily_cap` requests or the
    /// minute `per_minute` (atomic, so concurrent requests on any number of servers never exceed either); a
    /// refusal is counted instead.
    async fn count_connection_request(
        &self,
        org: OrgId,
        id: ConnectionId,
        daily_cap: Option<u32>,
        per_minute: Option<u32>,
        now: DateTime<Utc>,
    ) -> Res<Counted>;
    /// Today's counters for every connection of the org that made requests.
    async fn connection_usage(&self, org: OrgId, day: NaiveDate) -> Res<Vec<ConnectionUsage>>;
}
