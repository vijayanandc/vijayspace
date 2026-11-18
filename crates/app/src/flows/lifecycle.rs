//! Use cases: validate, publish, enable and disable.

use weft_domain::{
    error_codes,
    flow::{Issue, has_errors, validate},
    ids::FlowId,
    model::{FlowVersion, VersionKind},
};

use super::manage::{FlowView, get_flow, load};
use crate::tenancy::Actor;
use crate::{AppError, Deps, triggers::sync_registration};
use weft_domain::permissions::Permission;

/// Validates the current draft.
pub async fn validate_draft(
    deps: &Deps,
    actor: &Actor,
    id: FlowId,
) -> Result<Vec<Issue>, AppError> {
    actor.require(Permission::FlowView)?;
    let org = actor.org;
    let flow = load(deps, org, id).await?;
    Ok(validate(&flow.draft, deps.connectors.as_ref()))
}

/// Publishes the draft as a new immutable version; hot-swaps the trigger when enabled.
pub async fn publish_flow(
    deps: &Deps,
    actor: &Actor,
    id: FlowId,
) -> Result<(FlowVersion, Vec<Issue>), AppError> {
    actor.require(Permission::FlowEdit)?;
    let org = actor.org;
    let flow = load(deps, org, id).await?;
    let issues = validate(&flow.draft, deps.connectors.as_ref());
    if has_errors(&issues) {
        let count = issues
            .iter()
            .filter(|i| i.severity == weft_domain::flow::Severity::Error)
            .count();
        return Err(AppError::validation(
            error_codes::FLOW_INVALID,
            format!("Fix {count} issue(s) before publishing"),
        )
        .with_issues(issues));
    }
    let now = deps.clock.now();
    let mut version = FlowVersion {
        id: deps.new_version_id(),
        org_id: org,
        flow_id: id,
        kind: VersionKind::Published,
        version: None,
        definition: flow.draft.clone(),
        created_at: now,
    };
    version.version = Some(deps.store.publish_version(&version, now).await?);
    let flow = load(deps, org, id).await?;
    sync_registration(deps, &flow, Some(&version.definition)).await?;
    Ok((version, issues))
}

/// Turns a published flow on or off.
pub async fn set_enabled(
    deps: &Deps,
    actor: &Actor,
    id: FlowId,
    enabled: bool,
) -> Result<FlowView, AppError> {
    actor.require(Permission::FlowEdit)?;
    let org = actor.org;
    let flow = load(deps, org, id).await?;
    let published = match flow.published_version_id {
        Some(v) => deps.store.get_version(v).await?,
        None => None,
    };
    if enabled && published.is_none() {
        return Err(AppError::conflict(
            error_codes::FLOW_NOT_PUBLISHED,
            "Publish the flow before turning it on",
        ));
    }
    deps.store
        .set_enabled(org, id, enabled, deps.clock.now())
        .await?;
    let flow = load(deps, org, id).await?;
    sync_registration(deps, &flow, published.as_ref().map(|v| &v.definition)).await?;
    get_flow(deps, actor, id).await
}
