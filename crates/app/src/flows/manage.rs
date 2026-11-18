//! Use cases: create, read, list, rename, delete flows; save drafts; listen for samples.

use chrono::Duration;
use serde_json::{Value, json};
use weft_connector_sdk::oauth::random_token;
use weft_domain::{
    error_codes,
    flow::{FlowDefinition, Issue, TriggerNode, WebhookTrigger, validate},
    ids::{FlowId, OrgId},
    model::{Flow, FlowVersion, RegistrationKind, TriggerRegistration},
};

use crate::tenancy::Actor;
use crate::{AppError, Deps, ports::FlowListRow, triggers::registration_kind};
use weft_domain::permissions::Permission;

/// A flow plus its trigger registration (what the API shows).
#[derive(Debug, Clone)]
pub struct FlowView {
    pub flow: Flow,
    pub registration: TriggerRegistration,
}

fn not_found() -> AppError {
    AppError::not_found(error_codes::FLOW_NOT_FOUND, "Flow not found")
}

pub(crate) async fn load(deps: &Deps, org: OrgId, id: FlowId) -> Result<Flow, AppError> {
    deps.store.get_flow(org, id).await?.ok_or_else(not_found)
}

pub(crate) async fn registration(
    deps: &Deps,
    flow: FlowId,
) -> Result<TriggerRegistration, AppError> {
    deps.store
        .get_registration_by_flow(flow)
        .await?
        .ok_or_else(|| AppError::internal(format!("flow {flow} has no registration")))
}

/// Creates a flow (webhook trigger by default) with its trigger registration.
pub async fn create_flow(
    deps: &Deps,
    actor: &Actor,
    name: &str,
    description: Option<&str>,
    definition: Option<FlowDefinition>,
) -> Result<FlowView, AppError> {
    actor.require(Permission::FlowEdit)?;
    let org = actor.org;
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::validation(
            error_codes::VALIDATION_FAILED,
            "Name is required",
        ));
    }
    let now = deps.clock.now();
    let draft = definition
        .unwrap_or_else(|| FlowDefinition::new(TriggerNode::Webhook(WebhookTrigger::default())));
    let flow = Flow {
        id: deps.new_flow_id(),
        org_id: org,
        name: name.chars().take(200).collect(),
        description: description.map(str::to_owned),
        is_enabled: false,
        draft: draft.clone(),
        draft_revision: 1,
        published_version_id: None,
        published_version: None,
        created_at: now,
        updated_at: now,
        created_by: actor.user,
    };
    let registration = TriggerRegistration {
        id: deps.new_registration_id(),
        org_id: org,
        flow_id: flow.id,
        kind: registration_kind(&draft, &deps.connectors),
        token: random_token(),
        secret: deps.random_hex32(),
        is_active: false,
        listening_until: None,
        next_fire_at: None,
        state: json!({}),
        sample: None,
    };
    deps.store.insert_flow(&flow, &registration).await?;
    Ok(FlowView { flow, registration })
}

pub async fn get_flow(deps: &Deps, actor: &Actor, id: FlowId) -> Result<FlowView, AppError> {
    actor.require(Permission::FlowView)?;
    let org = actor.org;
    let flow = load(deps, org, id).await?;
    let registration = registration(deps, id).await?;
    Ok(FlowView { flow, registration })
}

pub async fn list_flows(deps: &Deps, actor: &Actor) -> Result<Vec<FlowListRow>, AppError> {
    actor.require(Permission::FlowView)?;
    let org = actor.org;
    deps.store.list_flows(org).await
}

pub async fn update_flow(
    deps: &Deps,
    actor: &Actor,
    id: FlowId,
    name: Option<&str>,
    description: Option<Option<&str>>,
) -> Result<FlowView, AppError> {
    actor.require(Permission::FlowEdit)?;
    let org = actor.org;
    let flow = load(deps, org, id).await?;
    let name = name
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .unwrap_or(&flow.name)
        .to_owned();
    let description = description.map_or(flow.description.clone(), |d| d.map(str::to_owned));
    deps.store
        .update_flow_meta(org, id, &name, description.as_deref(), deps.clock.now())
        .await?;
    get_flow(deps, actor, id).await
}

/// Saves the draft (optimistic concurrency on `revision`) and returns validation issues.
pub async fn save_draft(
    deps: &Deps,
    actor: &Actor,
    id: FlowId,
    definition: &FlowDefinition,
    revision: u32,
) -> Result<(u32, Vec<Issue>), AppError> {
    actor.require(Permission::FlowEdit)?;
    let org = actor.org;
    load(deps, org, id).await?;
    let new_revision = deps
        .store
        .save_draft(org, id, definition, revision, deps.clock.now())
        .await?;
    Ok((new_revision, validate(definition, deps.connectors.as_ref())))
}

/// Soft-deletes a flow and deactivates its trigger.
pub async fn delete_flow(deps: &Deps, actor: &Actor, id: FlowId) -> Result<(), AppError> {
    let org = actor.org;
    let mut flow = load(deps, org, id).await?;
    actor.require_on(Permission::FlowDelete, flow.created_by)?;
    flow.is_enabled = false;
    crate::triggers::sync_registration(deps, &flow, None).await?;
    deps.store.soft_delete_flow(org, id, deps.clock.now()).await
}

pub async fn list_versions(
    deps: &Deps,
    actor: &Actor,
    id: FlowId,
) -> Result<Vec<FlowVersion>, AppError> {
    actor.require(Permission::FlowView)?;
    let org = actor.org;
    load(deps, org, id).await?;
    deps.store.list_versions(org, id).await
}

/// Captures the next webhook/app event as the trigger sample for five minutes.
pub async fn start_listening(deps: &Deps, actor: &Actor, id: FlowId) -> Result<FlowView, AppError> {
    actor.require(Permission::FlowEdit)?;
    let org = actor.org;
    load(deps, org, id).await?;
    let reg = registration(deps, id).await?;
    let now = deps.clock.now();
    deps.store
        .set_listening(reg.id, Some(now + Duration::minutes(5)), now)
        .await?;
    get_flow(deps, actor, id).await
}

/// Public URL of a registration for webhooks.
#[must_use]
pub fn webhook_url(deps: &Deps, reg: &TriggerRegistration) -> String {
    format!(
        "{}/hooks/{}",
        deps.config.public_url.trim_end_matches('/'),
        reg.token
    )
}

/// Public URL of a registration for app events.
#[must_use]
pub fn app_event_url(deps: &Deps, reg: &TriggerRegistration) -> String {
    format!(
        "{}/hooks/app/{}",
        deps.config.public_url.trim_end_matches('/'),
        reg.token
    )
}

/// The trigger kind to display for a flow: the published runtime kind, else the draft's.
#[must_use]
pub fn display_kind(deps: &Deps, view: &FlowView) -> RegistrationKind {
    if view.flow.published_version_id.is_some() {
        view.registration.kind
    } else {
        registration_kind(&view.flow.draft, &deps.connectors)
    }
}

/// Whether the draft differs from the published version.
pub async fn has_unpublished_changes(deps: &Deps, flow: &Flow) -> Result<bool, AppError> {
    match flow.published_version_id {
        None => Ok(true),
        Some(v) => Ok(deps
            .store
            .get_version(v)
            .await?
            .is_none_or(|ver| ver.definition != flow.draft)),
    }
}

/// Default sample for a trigger when nothing was captured yet.
#[must_use]
pub fn empty_sample() -> Value {
    json!({})
}
