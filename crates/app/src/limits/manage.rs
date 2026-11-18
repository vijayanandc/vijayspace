//! Use cases: read and change a connection's limits, and today's usage per connection.

use serde::Serialize;
use weft_connector_sdk::ConnectionLimits;
use weft_domain::{
    error_codes,
    ids::{ConnectionId, OrgId},
};

use crate::tenancy::Actor;
use crate::{AppError, Deps};
use weft_domain::permissions::Permission;

/// The largest value a limit may be set to.
const MAX_LIMIT: u32 = 10_000_000;

/// A connection's limits: the connector defaults, the connection's overrides and the result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LimitsView {
    pub defaults: ConnectionLimits,
    pub overrides: ConnectionLimits,
    pub effective: ConnectionLimits,
    pub usage: UsageView,
}

/// Today's requests through a connection against its daily budget.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UsageView {
    pub connection_id: ConnectionId,
    /// UTC day, `YYYY-MM-DD`.
    pub day: String,
    pub requests: u32,
    pub refused: u32,
    pub daily_limit: Option<u32>,
    /// Share of the daily budget used, in percent (rounded down); `None` without a budget.
    pub percent: Option<u32>,
    /// `ok`, `warning` (80% used) or `critical` (95% used: background work waits).
    pub level: &'static str,
}

fn not_found() -> AppError {
    AppError::not_found(error_codes::CONNECTION_NOT_FOUND, "Connection not found")
}

/// Connector defaults and the connection's overrides.
async fn defaults_and_overrides(
    deps: &Deps,
    org: OrgId,
    id: ConnectionId,
) -> Result<(ConnectionLimits, ConnectionLimits), AppError> {
    let stored = deps
        .store
        .get_connection(org, id)
        .await?
        .ok_or_else(not_found)?;
    let defaults = deps
        .connectors
        .get(&stored.connection.connector)
        .map(|c| c.default_limits())
        .unwrap_or_default();
    let overrides = deps.store.get_connection_limits(org, id).await?;
    Ok((defaults, overrides))
}

pub(crate) async fn effective_limits(
    deps: &Deps,
    org: OrgId,
    id: ConnectionId,
) -> Result<ConnectionLimits, AppError> {
    let (defaults, overrides) = defaults_and_overrides(deps, org, id).await?;
    Ok(defaults.with_overrides(&overrides))
}

pub(crate) fn usage_view(
    id: ConnectionId,
    day: &str,
    requests: u32,
    refused: u32,
    daily: Option<u32>,
) -> UsageView {
    let percent = daily
        .filter(|d| *d > 0)
        .map(|d| u32::try_from(u64::from(requests) * 100 / u64::from(d)).unwrap_or(u32::MAX));
    let level = match percent {
        Some(p) if p >= 95 => "critical",
        Some(p) if p >= 80 => "warning",
        Some(_) | None => "ok",
    };
    UsageView {
        connection_id: id,
        day: day.to_owned(),
        requests,
        refused,
        daily_limit: daily,
        percent,
        level,
    }
}

/// A connection's limits and today's usage.
pub async fn connection_limits(
    deps: &Deps,
    actor: &Actor,
    id: ConnectionId,
) -> Result<LimitsView, AppError> {
    actor.require(Permission::FlowView)?;
    let org = actor.org;
    let (defaults, overrides) = defaults_and_overrides(deps, org, id).await?;
    let effective = defaults.with_overrides(&overrides);
    let today = deps.clock.now().date_naive();
    let usage = deps.store.connection_usage(org, today).await?;
    let (requests, refused) = usage
        .iter()
        .find(|u| u.connection_id == id)
        .map_or((0, 0), |u| (u.requests, u.refused));
    Ok(LimitsView {
        defaults,
        overrides,
        effective,
        usage: usage_view(
            id,
            &today.to_string(),
            requests,
            refused,
            effective.daily_requests,
        ),
    })
}

/// Sets the connection's overrides (`None` = the connector default, `0` = no limit).
pub async fn set_connection_limits(
    deps: &Deps,
    actor: &Actor,
    id: ConnectionId,
    overrides: ConnectionLimits,
) -> Result<LimitsView, AppError> {
    actor.require(Permission::ConnectionCreate)?;
    let org = actor.org;
    let too_big = [
        overrides.requests_per_minute,
        overrides.max_concurrent,
        overrides.daily_requests,
    ]
    .into_iter()
    .flatten()
    .any(|n| n > MAX_LIMIT);
    if too_big {
        return Err(AppError::validation(
            error_codes::VALIDATION_FAILED,
            format!("Limits can be at most {MAX_LIMIT}"),
        ));
    }
    deps.store
        .get_connection(org, id)
        .await?
        .ok_or_else(not_found)?;
    deps.store
        .set_connection_limits(org, id, &overrides, deps.clock.now())
        .await?;
    deps.limiters.invalidate(id);
    connection_limits(deps, actor, id).await
}

/// Today's usage for every connection of the org (connections without requests show zero).
pub async fn usage_today(deps: &Deps, actor: &Actor) -> Result<Vec<UsageView>, AppError> {
    actor.require(Permission::FlowView)?;
    let org = actor.org;
    let today = deps.clock.now().date_naive();
    let counters = deps.store.connection_usage(org, today).await?;
    let mut out = Vec::new();
    for connection in deps.store.list_connections(org).await? {
        let effective = effective_limits(deps, org, connection.id).await?;
        let (requests, refused) = counters
            .iter()
            .find(|u| u.connection_id == connection.id)
            .map_or((0, 0), |u| (u.requests, u.refused));
        out.push(usage_view(
            connection.id,
            &today.to_string(),
            requests,
            refused,
            effective.daily_requests,
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_levels_warn_at_80_and_go_critical_at_95_percent() {
        let id = ConnectionId::from_uuid(uuid::Uuid::nil());
        assert_eq!(usage_view(id, "d", 79, 0, Some(100)).level, "ok");
        assert_eq!(usage_view(id, "d", 80, 0, Some(100)).level, "warning");
        assert_eq!(usage_view(id, "d", 95, 0, Some(100)).level, "critical");
        assert_eq!(usage_view(id, "d", 5, 0, None).percent, None);
        assert_eq!(usage_view(id, "d", 5, 0, None).level, "ok");
    }
}
