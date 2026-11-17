//! Firing due schedule triggers (exactly one execution per slot).

use chrono::{Duration, SecondsFormat};
use serde_json::json;
use weft_domain::{
    flow::TriggerNode,
    model::{ExecutionMode, TriggerRegistration},
};

use crate::{AppError, Deps, executions::start::new_execution, ports::NewTriggerEvent};

/// Fires due schedules; returns how many executions were created.
pub async fn fire_due_schedules(deps: &Deps) -> Result<usize, AppError> {
    let now = deps.clock.now();
    let due = deps
        .store
        .take_due(&["schedule"], now, now + Duration::minutes(5), 100)
        .await?;
    let mut created = 0;
    for reg in due {
        match fire_one(deps, &reg).await {
            Ok(n) => created += n,
            Err(e) => {
                tracing::warn!(registration = %reg.id, error = %e, "schedule fire failed; will retry after the lease");
            }
        }
    }
    Ok(created)
}

async fn fire_one(deps: &Deps, reg: &TriggerRegistration) -> Result<usize, AppError> {
    let now = deps.clock.now();
    let slot = reg.next_fire_at.unwrap_or(now);
    let Some(flow) = deps.store.get_flow_system(reg.flow_id).await? else {
        deps.store.set_next_fire(reg.id, None, now).await?;
        return Ok(0);
    };
    let version = match flow.published_version_id {
        Some(v) => deps.store.get_version(v).await?,
        None => None,
    };
    let (Some(version), true) = (version, reg.is_active && flow.is_enabled) else {
        deps.store.set_next_fire(reg.id, None, now).await?;
        return Ok(0);
    };
    let TriggerNode::Schedule(s) = &version.definition.trigger else {
        deps.store.set_next_fire(reg.id, None, now).await?;
        return Ok(0);
    };
    let slot_key = slot.to_rfc3339_opts(SecondsFormat::Secs, true);
    let data = json!({"scheduled_for": slot_key, "fired_at": now});
    let event = NewTriggerEvent {
        id: deps.new_event_id(),
        registration_id: reg.id,
        dedupe_key: Some(format!("slot:{slot_key}")),
        payload: data.clone(),
    };
    let item = new_execution(
        deps,
        flow.org_id,
        flow.id,
        version.id,
        ExecutionMode::Live,
        data,
        Some(event),
        now,
        None,
    );
    let created = deps
        .store
        .create_executions(flow.org_id, flow.id, &[item], now)
        .await?
        .len();
    // Misfire policy: after downtime fire once, then continue from now (older slots are skipped).
    let next = s.schedule.next_after(slot.max(now)).ok().flatten();
    deps.store.set_next_fire(reg.id, next, now).await?;
    deps.wake.notify_one();
    Ok(created)
}
