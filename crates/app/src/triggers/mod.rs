//! Triggers at runtime: registration sync, webhook and app-event ingress, schedules, polling,
//! and provider subscriptions.

mod inbound;
mod ingress;
mod items;
mod poll;
mod registration;
mod schedule;
mod subscriptions;

pub use ingress::{InboundHttp, IngressResult, ingest_app_event, ingest_webhook};

/// Starts executions for items outside an inbound request (backfill).
pub async fn ingress_items(
    deps: &crate::Deps,
    reg: &weft_domain::model::TriggerRegistration,
    flow: &weft_domain::model::Flow,
    version: &weft_domain::model::FlowVersion,
    items: &[weft_connector_sdk::EventItem],
    mode: weft_domain::model::ExecutionMode,
) -> Result<Vec<weft_domain::ids::ExecutionId>, crate::AppError> {
    items::start_items(
        deps,
        reg,
        flow,
        version,
        items,
        None,
        mode,
        deps.clock.now(),
    )
    .await
}
pub use poll::{enqueue_due_polls, run_poll};
pub use registration::{registration_kind, sync_registration};
pub use schedule::fire_due_schedules;
pub use subscriptions::renew_due_subscriptions;
