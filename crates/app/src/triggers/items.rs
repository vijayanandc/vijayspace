//! Turning trigger items into executions: per-version dedupe and per-record debounce.

use chrono::{DateTime, Duration, Utc};
use weft_connector_sdk::EventItem;
use weft_domain::{
    ids::ExecutionId,
    model::{ExecutionMode, Flow, FlowVersion, TriggerRegistration},
};

use crate::{
    AppError, Deps,
    executions::start::new_execution,
    ports::{NewExecution, NewTriggerEvent},
};

/// Creates executions for trigger items (with optional debounce per record).
#[allow(clippy::too_many_arguments)]
pub(crate) async fn start_items(
    deps: &Deps,
    reg: &TriggerRegistration,
    flow: &Flow,
    version: &FlowVersion,
    items: &[EventItem],
    debounce_secs: Option<u32>,
    mode: ExecutionMode,
    now: DateTime<Utc>,
) -> Result<Vec<ExecutionId>, AppError> {
    let debounce = debounce_secs.filter(|s| *s > 0);
    let new: Vec<NewExecution> = items
        .iter()
        .map(|item| {
            let event = |dedupe_key: Option<String>| NewTriggerEvent {
                id: deps.new_event_id(),
                registration_id: reg.id,
                dedupe_key,
                payload: item.data.clone(),
            };
            let burst = debounce.zip(item.record_id.as_ref()).map(|(secs, record)| {
                let window = i64::from(secs);
                let bucket = now.timestamp().div_euclid(window) + 1;
                let run_at = DateTime::from_timestamp(bucket * window, 0).unwrap_or(now);
                (
                    format!("rec:{record}:{bucket}"),
                    run_at.max(now + Duration::seconds(1)),
                )
            });
            let (marker, run_at) = match &burst {
                Some((key, at)) => (Some(key.clone()), *at),
                None => (Some(item.dedupe_key.clone()), now),
            };
            let mut exe = new_execution(
                deps,
                flow.org_id,
                flow.id,
                version.id,
                mode,
                item.data.clone(),
                Some(event(marker)),
                run_at,
                None,
            );
            if burst.is_some() {
                exe.seen = Some(event(Some(item.dedupe_key.clone())));
                exe.coalesce = true;
            }
            exe
        })
        .collect();
    let ids = deps
        .store
        .create_executions(flow.org_id, flow.id, &new, now)
        .await?;
    if !ids.is_empty() {
        deps.wake.notify_one();
    }
    Ok(ids)
}

/// Idempotency keys are stored in a 191-character column (with an item suffix): long keys are
/// replaced by their SHA-256, so any length dedupes and none can fail the insert.
pub(super) fn bounded_key(key: &str) -> String {
    if key.len() <= 128 {
        key.to_owned()
    } else {
        use sha2::{Digest as _, Sha256};
        format!("sha256:{}", hex::encode(Sha256::digest(key.as_bytes())))
    }
}

#[cfg(test)]
mod tests {
    use super::bounded_key;

    #[test]
    fn long_idempotency_keys_are_hashed_to_fit_the_column() {
        assert_eq!(bounded_key("order-42"), "order-42");
        let long = "k".repeat(500);
        let hashed = bounded_key(&long);
        assert_eq!(hashed.len(), "sha256:".len() + 64);
        assert_eq!(
            hashed,
            bounded_key(&long),
            "stable, so retries still dedupe"
        );
    }
}
