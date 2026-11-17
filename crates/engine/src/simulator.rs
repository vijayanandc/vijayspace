//! A pure driver that runs a plan to completion with scripted action results (tests only).

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use serde_json::{Map, Value, json};

use crate::{
    AfterResult, Directive, Facts, Outcome, Plan, RunState, StepError, StepResult, advance,
    continued_error_output, record_result,
};

/// Scripted result for the next action call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scripted {
    Ok,
    Retryable,
    Permanent,
}

/// Summary of a simulated run.
#[derive(Debug, Clone, PartialEq)]
pub struct SimRun {
    /// `(step, attempt)` for every action call, in order.
    pub actions: Vec<(String, u16)>,
    /// Steps that succeeded, in order.
    pub successes: Vec<String>,
    pub outcome: Outcome,
    pub final_state: RunState,
}

/// Drives `advance`/`record_result` like the real runner, but in memory.
#[derive(Debug, Clone)]
pub struct Simulator<'a> {
    plan: &'a Plan,
    script: Vec<Scripted>,
    reload_at: BTreeSet<usize>,
    pub now: DateTime<Utc>,
}

impl<'a> Simulator<'a> {
    #[must_use]
    pub fn new(plan: &'a Plan, script: Vec<Scripted>, now: DateTime<Utc>) -> Self {
        Self {
            plan,
            script,
            reload_at: BTreeSet::new(),
            now,
        }
    }

    /// Serialize and deserialize the state after these step counts (simulates crash + resume).
    #[must_use]
    pub fn reload_state_at(mut self, points: &BTreeSet<usize>) -> Self {
        self.reload_at.clone_from(points);
        self
    }

    /// Runs until the execution finishes (or a safety bound is hit, reported as a failure).
    pub fn run_to_end(mut self) -> SimRun {
        let trigger = json!({"n": 1});
        let meta = json!({"flow": {"name": "sim"}, "execution": {"id": "exe_sim"}});
        let mut outputs: Map<String, Value> = Map::new();
        let mut state = RunState::default();
        let mut run = SimRun {
            actions: Vec::new(),
            successes: Vec::new(),
            outcome: Outcome::Succeeded,
            final_state: RunState::default(),
        };
        let mut script = std::mem::take(&mut self.script).into_iter();
        for tick in 0..100_000 {
            if self.reload_at.contains(&tick) {
                let text = serde_json::to_string(&state).unwrap_or_default();
                state = serde_json::from_str(&text).unwrap_or_default();
            }
            let facts = Facts {
                trigger: &trigger,
                outputs: &outputs,
                meta: &meta,
                now: self.now,
            };
            let adv = match advance(self.plan, state.clone(), &facts) {
                Ok(a) => a,
                Err(e) => {
                    return finish(
                        run,
                        state,
                        Outcome::Failed {
                            code: "engine".into(),
                            message: e.to_string(),
                            step: None,
                        },
                    );
                }
            };
            state = adv.state;
            match adv.directive {
                Directive::Finish(outcome) => return finish(run, state, outcome),
                Directive::Sleep { until, .. } => self.now = until,
                Directive::RunAction { step, attempt, .. } => {
                    run.actions.push((step.clone(), attempt));
                    let result = match script.next().unwrap_or(Scripted::Ok) {
                        Scripted::Ok => StepResult::Succeeded(json!({"ok": step})),
                        kind @ (Scripted::Retryable | Scripted::Permanent) => {
                            StepResult::Failed(StepError {
                                code: "app.error".into(),
                                message: "scripted".into(),
                                retryable: kind == Scripted::Retryable,
                                retry_after_secs: None,
                                details: Value::Null,
                            })
                        }
                    };
                    let (next, after) =
                        match record_result(self.plan, state.clone(), &step, &result, self.now) {
                            Ok(r) => r,
                            Err(e) => {
                                return finish(
                                    run,
                                    state,
                                    Outcome::Failed {
                                        code: "engine".into(),
                                        message: e.to_string(),
                                        step: None,
                                    },
                                );
                            }
                        };
                    state = next;
                    match (after, &result) {
                        (AfterResult::Continue, StepResult::Succeeded(out)) => {
                            outputs.insert(step.clone(), out.clone());
                            run.successes.push(step);
                        }
                        (AfterResult::Continue, StepResult::Failed(err)) => {
                            outputs.insert(step, continued_error_output(err));
                        }
                        (AfterResult::RetryAt(t), _) => self.now = t,
                        (AfterResult::FailExecution(err), _) => {
                            return finish(
                                run,
                                state,
                                Outcome::Failed {
                                    code: err.code,
                                    message: err.message,
                                    step: Some(step),
                                },
                            );
                        }
                    }
                }
            }
        }
        finish(
            run,
            state,
            Outcome::Failed {
                code: "simulator.bound".into(),
                message: "did not terminate".into(),
                step: None,
            },
        )
    }
}

fn finish(mut run: SimRun, state: RunState, outcome: Outcome) -> SimRun {
    run.outcome = outcome;
    run.final_state = state;
    run
}
