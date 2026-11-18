//! Prebuilt integrations (recipes): flow templates with connection slots, installed as drafts.

mod crm_fsm;
mod forma;
mod hms;

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;
use weft_domain::{
    error_codes,
    flow::FlowDefinition,
    ids::{ConnectionId, FlowId, OrgId},
};

use crate::tenancy::Actor;
use crate::{AppError, Deps, flows::create_flow};
use weft_domain::permissions::Permission;

/// Placeholder prefix in templates: the string `"$slot:crm"` becomes the `crm` connection ID.
const SLOT_PREFIX: &str = "$slot:";

/// A connection the installer must choose.
#[derive(Debug, Clone, Serialize)]
pub struct Slot {
    pub slot: &'static str,
    pub connector: &'static str,
    pub label: &'static str,
}

/// One flow a recipe creates.
#[derive(Debug, Clone, Serialize)]
pub struct FlowTemplate {
    pub name: &'static str,
    pub description: &'static str,
    #[serde(skip)]
    pub definition: Value,
}

/// A recipe as listed by the API.
#[derive(Debug, Clone, Serialize)]
pub struct Recipe {
    pub key: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub apps: Vec<&'static str>,
    pub slots: Vec<Slot>,
    pub flows: Vec<FlowTemplate>,
}

/// All recipes, in catalog order.
#[must_use]
pub fn catalog() -> Vec<Recipe> {
    let mut all = crm_fsm::recipes();
    all.extend(hms::recipes());
    all.extend(forma::recipes());
    all
}

/// Creates the recipe's flows as drafts, with each slot bound to the chosen connection.
/// Everything is checked before the first flow is created.
pub async fn install(
    deps: &Deps,
    actor: &Actor,
    key: &str,
    connections: &BTreeMap<String, String>,
) -> Result<Vec<FlowId>, AppError> {
    actor.require(Permission::FlowEdit)?;
    let org = actor.org;
    let recipe = catalog()
        .into_iter()
        .find(|r| r.key == key)
        .ok_or_else(|| {
            AppError::not_found(
                error_codes::RECIPE_NOT_FOUND,
                format!("Unknown recipe `{key}`"),
            )
        })?;
    for slot in &recipe.slots {
        check_slot(deps, org, slot, connections.get(slot.slot)).await?;
    }
    let mut drafts = Vec::with_capacity(recipe.flows.len());
    for template in &recipe.flows {
        let bound = bind_slots(&template.definition, connections);
        let definition: FlowDefinition = serde_json::from_value(bound)
            .map_err(|e| AppError::internal(format!("recipe {key}: bad template: {e}")))?;
        drafts.push((template, definition));
    }
    let mut ids = Vec::with_capacity(drafts.len());
    for (template, definition) in drafts {
        let view = create_flow(
            deps,
            actor,
            template.name,
            Some(template.description),
            Some(definition),
        )
        .await?;
        ids.push(view.flow.id);
    }
    Ok(ids)
}

async fn check_slot(
    deps: &Deps,
    org: OrgId,
    slot: &Slot,
    chosen: Option<&String>,
) -> Result<(), AppError> {
    let missing = || {
        AppError::validation(
            error_codes::RECIPE_SLOT_MISSING,
            format!("Choose a connection for `{}` ({})", slot.slot, slot.label),
        )
    };
    let id: ConnectionId = chosen.ok_or_else(missing)?.parse().map_err(|_| missing())?;
    let stored = deps.store.get_connection(org, id).await?.ok_or_else(|| {
        AppError::not_found(
            error_codes::CONNECTION_NOT_FOUND,
            format!("Connection for `{}` not found", slot.slot),
        )
    })?;
    if stored.connection.connector != slot.connector {
        return Err(AppError::validation(
            error_codes::RECIPE_SLOT_MISMATCH,
            format!(
                "`{}` needs a {} connection, not {}",
                slot.slot, slot.connector, stored.connection.connector
            ),
        ));
    }
    Ok(())
}

/// Replaces every `"$slot:<name>"` string with the chosen connection ID.
fn bind_slots(value: &Value, connections: &BTreeMap<String, String>) -> Value {
    match value {
        Value::String(s) => s
            .strip_prefix(SLOT_PREFIX)
            .and_then(|slot| connections.get(slot))
            .map_or_else(|| value.clone(), |id| Value::String(id.clone())),
        Value::Array(items) => {
            Value::Array(items.iter().map(|v| bind_slots(v, connections)).collect())
        }
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), bind_slots(v, connections)))
                .collect(),
        ),
        Value::Null | Value::Bool(_) | Value::Number(_) => value.clone(),
    }
}

/// Shared template pieces.
pub(crate) mod parts {
    use serde_json::{Value, json};

    /// Zoho CRM record trigger with reconciliation polling and a 30 s debounce.
    pub fn zoho_trigger(module: &str, events: &[&str]) -> Value {
        json!({"type": "app", "connector": "zoho_crm", "trigger": "record_event",
            "connection_id": "$slot:crm", "input": {"module": module, "events": events},
            "poll_interval_secs": 3600, "debounce_secs": 30})
    }

    /// Appblocks record webhook trigger with reconciliation polling and a 30 s debounce.
    pub fn appblocks_trigger(slot: &str, module: &str, operation: &str) -> Value {
        json!({"type": "app", "connector": "appblocks", "trigger": "record_event",
            "connection_id": format!("$slot:{slot}"), "input": {"module": module, "operation": operation},
            "poll_interval_secs": 3600, "debounce_secs": 30})
    }

    /// A `core.sync` `upsert_linked_record` step.
    #[allow(clippy::too_many_arguments)]
    pub fn upsert(
        key: &str,
        link_set: &str,
        source: (&str, &str),
        target: (&str, &str),
        values: Value,
        match_on: Option<(&str, &str)>,
        lookups: Value,
    ) -> Value {
        let mut input = json!({
            "link_set": link_set,
            "source_connection_id": format!("$slot:{}", source.0), "source_object": source.1,
            "source_record_id": "{{ trigger.id }}",
            "target_connection_id": format!("$slot:{}", target.0), "target_object": target.1,
            "values": values,
        });
        if let Some((field, value)) = match_on {
            input["match_field"] = json!(field);
            input["match_value"] = json!(value);
        }
        if lookups.as_array().is_some_and(|l| !l.is_empty()) {
            input["lookups"] = lookups;
        }
        json!({"key": key, "type": "action", "connector": "core.sync", "action": "upsert_linked_record", "input": input})
    }

    /// A `core.sync` `find_linked_record` step (target record → source record).
    pub fn find_source(key: &str, link_set: &str, target: (&str, &str), target_id: &str) -> Value {
        json!({"key": key, "type": "action", "connector": "core.sync", "action": "find_linked_record",
            "input": {"link_set": link_set,
                "target_connection_id": format!("$slot:{}", target.0), "target_object": target.1,
                "target_record_id": target_id}})
    }

    /// A decision with one branch; the rest of the flow runs only when it matches.
    pub fn only_if(key: &str, name: &str, condition: Value, steps: Value) -> Value {
        json!({"key": key, "type": "decision",
            "branches": [{"key": format!("{key}_yes"), "name": name, "condition": condition, "steps": steps}],
            "otherwise": []})
    }

    /// A comparison leaf.
    pub fn cmp(left: &str, op: &str, right: Value) -> Value {
        json!({"left": left, "op": op, "right": right})
    }
}

#[cfg(test)]
mod tests;
