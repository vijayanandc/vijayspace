//! Recipe catalog invariants: unique keys, every slot bound, templates parse, R6 allowlist.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;
use weft_domain::flow::{FlowDefinition, Step, StepKind};

use super::{SLOT_PREFIX, bind_slots, catalog, hms::PATIENT_ALLOWLIST};

fn slot_refs(value: &Value, out: &mut BTreeSet<String>) {
    match value {
        Value::String(s) => {
            if let Some(slot) = s.strip_prefix(SLOT_PREFIX) {
                out.insert(slot.to_owned());
            }
        }
        Value::Array(items) => items.iter().for_each(|v| slot_refs(v, out)),
        Value::Object(map) => map.values().for_each(|v| slot_refs(v, out)),
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

#[test]
fn recipe_keys_are_unique_and_mvp_recipes_r1_to_r9_exist() {
    let keys: Vec<_> = catalog().iter().map(|r| r.key).collect();
    let unique: BTreeSet<_> = keys.iter().collect();
    assert_eq!(keys.len(), unique.len());
    for n in 1..=9 {
        assert!(
            keys.iter().any(|k| k.starts_with(&format!("r{n}_"))),
            "missing R{n}"
        );
    }
}

#[test]
fn every_slot_used_by_a_template_is_declared_and_every_declared_slot_is_used() {
    for recipe in catalog() {
        let declared: BTreeSet<String> = recipe.slots.iter().map(|s| s.slot.to_owned()).collect();
        let mut used = BTreeSet::new();
        for flow in &recipe.flows {
            slot_refs(&flow.definition, &mut used);
        }
        assert_eq!(declared, used, "{}", recipe.key);
    }
}

#[test]
fn templates_parse_once_slots_are_bound() {
    for recipe in catalog() {
        let ids: BTreeMap<String, String> = recipe
            .slots
            .iter()
            .map(|s| (s.slot.to_owned(), format!("cxn_{}", s.slot)))
            .collect();
        for flow in &recipe.flows {
            let bound = bind_slots(&flow.definition, &ids);
            let mut left = BTreeSet::new();
            slot_refs(&bound, &mut left);
            assert!(left.is_empty(), "{}: unbound {left:?}", recipe.key);
            let def: Result<FlowDefinition, _> = serde_json::from_value(bound);
            assert!(
                def.is_ok(),
                "{} / {}: {:?}",
                recipe.key,
                flow.name,
                def.err()
            );
        }
    }
}

fn upsert_values(steps: &[Step], out: &mut Vec<Value>) {
    for step in steps {
        match &step.kind {
            StepKind::Action(a) if a.action == "upsert_linked_record" => {
                out.push(a.input["values"].clone());
            }
            StepKind::Decision(d) => {
                for b in &d.branches {
                    upsert_values(&b.steps, out);
                }
                upsert_values(&d.otherwise, out);
            }
            StepKind::Action(_)
            | StepKind::Delay(_)
            | StepKind::SetVariable(_)
            | StepKind::Stop(_) => {}
        }
    }
}

#[test]
fn r6_sends_only_allowlisted_patient_fields_and_only_with_consent() {
    let recipe = catalog()
        .into_iter()
        .find(|r| r.key == "r6_hms_to_crm")
        .unwrap();
    let patients = &recipe.flows[0].definition;
    let def: FlowDefinition = serde_json::from_value(patients.clone()).unwrap();
    let StepKind::Decision(gate) = &def.steps[0].kind else {
        panic!("R6 must start with the consent decision");
    };
    assert_eq!(
        def.steps.len(),
        1,
        "nothing may run outside the consent gate"
    );
    assert!(gate.otherwise.is_empty());
    let mut values = Vec::new();
    upsert_values(&def.steps, &mut values);
    assert_eq!(values.len(), 1);
    for key in values[0].as_object().unwrap().keys() {
        assert!(
            PATIENT_ALLOWLIST.iter().any(|(f, _)| f == key),
            "{key} is not allowlisted"
        );
    }
}

#[test]
fn bind_slots_leaves_unknown_placeholders_and_other_values_alone() {
    let ids = BTreeMap::from([("crm".to_owned(), "cxn_1".to_owned())]);
    let v = serde_json::json!({"a": "$slot:crm", "b": ["$slot:other", 1, true, null], "c": "x"});
    assert_eq!(
        bind_slots(&v, &ids),
        serde_json::json!({"a": "cxn_1", "b": ["$slot:other", 1, true, null], "c": "x"})
    );
}
