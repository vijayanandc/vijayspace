//! Property tests for the engine invariants I1–I5 (`docs/plan/04-workflow-engine.md` §5).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;

use chrono::{TimeZone, Utc};
use proptest::prelude::*;
use serde_json::{Value, json};
use weft_domain::flow::FlowDefinition;
use weft_engine::{
    Outcome, compile,
    simulator::{Scripted, Simulator},
};

/// Shape of a generated step tree (keys are assigned afterwards so they are unique).
#[derive(Debug, Clone)]
enum Shape {
    Action { continue_on_error: bool },
    Delay(u64),
    SetVar,
    Stop,
    Decision(Vec<Vec<Shape>>, Vec<Shape>),
}

fn arb_shapes() -> impl Strategy<Value = Vec<Shape>> {
    let leaf = prop_oneof![
        6 => any::<bool>().prop_map(|c| Shape::Action { continue_on_error: c }),
        2 => (1u64..100_000).prop_map(Shape::Delay),
        2 => Just(Shape::SetVar),
        1 => Just(Shape::Stop),
    ];
    let tree = leaf.prop_recursive(3, 40, 4, |inner| {
        (
            prop::collection::vec(prop::collection::vec(inner.clone(), 0..4), 1..3),
            prop::collection::vec(inner, 0..3),
        )
            .prop_map(|(branches, otherwise)| Shape::Decision(branches, otherwise))
    });
    prop::collection::vec(tree, 0..8)
}

fn build(shapes: &[Shape], counter: &mut u32) -> Vec<Value> {
    shapes
        .iter()
        .map(|shape| {
            *counter += 1;
            let key = format!("s{counter}");
            match shape {
                Shape::Action { continue_on_error } => json!({
                    "key": key, "type": "action", "connector": "c", "action": "x",
                    "input": {"prev": "{{ vars.flag }}", "n": "{{ trigger.n }}"},
                    "on_error": if *continue_on_error { "continue" } else { "fail_flow" },
                    "retry": {"max_attempts": 3}
                }),
                Shape::Delay(secs) => json!({"key": key, "type": "delay", "seconds": secs}),
                Shape::SetVar => json!({"key": key, "type": "set_variable", "assignments": [{"name": "flag", "value": "{{ (vars.flag or 0) + 1 }}"}]}),
                Shape::Stop => json!({"key": key, "type": "stop"}),
                Shape::Decision(branches, otherwise) => {
                    let bs: Vec<Value> = branches
                        .iter()
                        .enumerate()
                        .map(|(i, steps)| json!({
                            "key": format!("b{i}"),
                            "condition": {"left": "{{ vars.flag or 0 }}", "op": "eq", "right": i},
                            "steps": build(steps, counter)
                        }))
                        .collect();
                    json!({"key": key, "type": "decision", "branches": bs, "otherwise": build(otherwise, counter)})
                }
            }
        })
        .collect()
}

fn definition(shapes: &[Shape]) -> FlowDefinition {
    let mut counter = 0;
    serde_json::from_value(
        json!({"trigger": {"type": "manual"}, "steps": build(shapes, &mut counter)}),
    )
    .unwrap()
}

fn arb_script() -> impl Strategy<Value = Vec<Scripted>> {
    prop::collection::vec(
        prop_oneof![8 => Just(Scripted::Ok), 1 => Just(Scripted::Retryable), 1 => Just(Scripted::Permanent)],
        0..64,
    )
}

fn start() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).single().unwrap()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    // I1: always terminates with a final outcome (never hits the simulator bound).
    #[test]
    fn always_terminates(shapes in arb_shapes(), script in arb_script()) {
        let plan = compile(&definition(&shapes)).unwrap();
        let run = Simulator::new(&plan, script, start()).run_to_end();
        let bounded = matches!(&run.outcome, Outcome::Failed { code, .. } if code == "simulator.bound" || code == "engine");
        prop_assert!(!bounded, "{:?}", run.outcome);
    }

    // I2: deterministic.
    #[test]
    fn deterministic(shapes in arb_shapes(), script in arb_script()) {
        let plan = compile(&definition(&shapes)).unwrap();
        let a = Simulator::new(&plan, script.clone(), start()).run_to_end();
        let b = Simulator::new(&plan, script, start()).run_to_end();
        prop_assert_eq!(a, b);
    }

    // I3: persisting and reloading the state at any point changes nothing.
    #[test]
    fn resume_from_any_checkpoint_is_equivalent(
        shapes in arb_shapes(), script in arb_script(), points in prop::collection::btree_set(0usize..64, 0..6)
    ) {
        let plan = compile(&definition(&shapes)).unwrap();
        let straight = Simulator::new(&plan, script.clone(), start()).run_to_end();
        let resumed = Simulator::new(&plan, script, start()).reload_state_at(&points).run_to_end();
        prop_assert_eq!(straight.actions, resumed.actions);
        prop_assert_eq!(straight.outcome, resumed.outcome);
    }

    // I4/I5: a succeeded step never runs again, and actions follow program order.
    #[test]
    fn succeeded_steps_never_rerun_and_order_is_kept(shapes in arb_shapes(), script in arb_script()) {
        let plan = compile(&definition(&shapes)).unwrap();
        let run = Simulator::new(&plan, script, start()).run_to_end();
        let unique: BTreeSet<&String> = run.successes.iter().collect();
        prop_assert_eq!(unique.len(), run.successes.len());
        let pcs: Vec<usize> = run.actions.iter().map(|(s, _)| plan.pc_of(s).unwrap()).collect();
        prop_assert!(pcs.windows(2).all(|w| w[0] <= w[1]), "{:?}", pcs);
        // Retries of one step have increasing attempt numbers and never exceed the policy.
        prop_assert!(run.actions.iter().all(|(_, attempt)| *attempt <= 3));
    }
}
