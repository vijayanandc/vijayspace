//! The pure interpreter: given a plan, the run state and read-only facts, decide what happens next.
//!
//! No IO, no clock, no randomness: `now` is passed in. The runner (imperative shell) performs
//! the returned directive and persists the new state. See `docs/plan/04-workflow-engine.md` §5.

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use weft_domain::{
    error_codes,
    flow::{OnError, Step, StepKind, StopStatus},
};
use weft_expr::{eval_condition, parse_datetime, render_text, render_value};

use crate::plan::{Instr, Pc, Plan};

/// Guard against runaway executions.
pub const MAX_INSTRUCTIONS: u32 = 10_000;
/// Default attempts per action (1 try + 8 automatic retries, like Zoho Flow).
pub const DEFAULT_MAX_ATTEMPTS: u16 = 9;
/// Retry backoff schedule in seconds (attempt 1 failed → wait 60 s, …), capped at 6 h.
pub const BACKOFF_SECS: [i64; 8] = [60, 300, 900, 1_800, 3_600, 7_200, 14_400, 21_600];

/// Persisted interpreter state of one execution.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RunState {
    pub pc: Pc,
    #[serde(default)]
    pub vars: Map<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait: Option<Wait>,
    #[serde(default)]
    pub attempts: BTreeMap<String, u16>,
    #[serde(default)]
    pub executed: u32,
}

/// An in-progress delay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wait {
    pub step: String,
    pub until: DateTime<Utc>,
}

/// Read-only inputs for one `advance` call.
#[derive(Debug, Clone, Copy)]
pub struct Facts<'a> {
    /// Trigger output (`trigger` in templates).
    pub trigger: &'a Value,
    /// Latest output of each completed step (`steps.<key>`).
    pub outputs: &'a Map<String, Value>,
    /// `{"flow": {...}, "execution": {...}}` metadata.
    pub meta: &'a Value,
    pub now: DateTime<Utc>,
}

/// What the runner must do next.
#[derive(Debug, Clone, PartialEq)]
pub enum Directive {
    /// Call a connector action with already-rendered input.
    RunAction {
        step: String,
        attempt: u16,
        input: Value,
    },
    /// Persist state and resume at `until`.
    Sleep { step: String, until: DateTime<Utc> },
    /// The execution is over.
    Finish(Outcome),
}

/// Final outcome of an execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Outcome {
    Succeeded,
    Failed {
        code: String,
        message: String,
        step: Option<String>,
    },
}

/// Logic events the runner records as step runs so history shows decisions too.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum TraceEntry {
    Decision {
        step: String,
        branch: Option<String>,
        warnings: Vec<String>,
    },
    VariablesSet {
        step: String,
        values: Map<String, Value>,
    },
    DelayScheduled {
        step: String,
        until: DateTime<Utc>,
    },
    DelayCompleted {
        step: String,
    },
    StepFailed {
        step: String,
        code: String,
        message: String,
        continued: bool,
    },
    Stopped {
        step: String,
        status: StopStatus,
        message: Option<String>,
    },
}

/// Result of `advance`.
#[derive(Debug, Clone, PartialEq)]
pub struct Advance {
    pub state: RunState,
    pub directive: Directive,
    pub trace: Vec<TraceEntry>,
}

/// An action's failure, classified by the connector.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_after_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub details: Value,
}

/// Outcome of one action attempt.
#[derive(Debug, Clone, PartialEq)]
pub enum StepResult {
    Succeeded(Value),
    Failed(StepError),
}

/// What happens after an action result was recorded.
#[derive(Debug, Clone, PartialEq)]
pub enum AfterResult {
    /// Call `advance` again (the step succeeded, or failed with `on_error: continue`).
    Continue,
    /// Retry the same step at this time.
    RetryAt(DateTime<Utc>),
    /// The execution fails.
    FailExecution(StepError),
}

/// Internal inconsistency between a plan and a state (a bug or a corrupted state).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EngineError {
    #[error("program counter {0} is out of range")]
    PcOutOfRange(Pc),
    #[error("step `{0}` is not in the plan")]
    UnknownStep(String),
    #[error("expected action `{expected}` at the program counter, found {found}")]
    NotAtAction { expected: String, found: String },
}

fn scope(facts: &Facts<'_>, state: &RunState) -> Value {
    json!({
        "trigger": facts.trigger,
        "steps": facts.outputs,
        "vars": state.vars,
        "flow": facts.meta.get("flow").cloned().unwrap_or(Value::Null),
        "execution": facts.meta.get("execution").cloned().unwrap_or(Value::Null),
    })
}

fn step_of<'p>(plan: &'p Plan, key: &str) -> Result<&'p Step, EngineError> {
    plan.step(key)
        .ok_or_else(|| EngineError::UnknownStep(key.to_owned()))
}

fn finish(state: RunState, trace: Vec<TraceEntry>, outcome: Outcome) -> Advance {
    Advance {
        state,
        directive: Directive::Finish(outcome),
        trace,
    }
}

fn failed(code: &str, message: impl Into<String>, step: Option<&str>) -> Outcome {
    Outcome::Failed {
        code: code.to_owned(),
        message: message.into(),
        step: step.map(str::to_owned),
    }
}

/// Runs control-flow instructions until something needs the outside world. Pure.
pub fn advance(
    plan: &Plan,
    mut state: RunState,
    facts: &Facts<'_>,
) -> Result<Advance, EngineError> {
    let mut trace = Vec::new();
    loop {
        if state.executed >= MAX_INSTRUCTIONS {
            let outcome = failed(
                error_codes::ENGINE_BUDGET_EXCEEDED,
                "The execution exceeded its instruction budget",
                None,
            );
            return Ok(finish(state, trace, outcome));
        }
        state.executed += 1;
        let instr = plan
            .instrs
            .get(state.pc)
            .ok_or(EngineError::PcOutOfRange(state.pc))?;
        match instr {
            Instr::End => return Ok(finish(state, trace, Outcome::Succeeded)),
            Instr::Jump { target } => state.pc = *target,
            Instr::Branch {
                step,
                arms,
                otherwise,
            } => {
                let sc = scope(facts, &state);
                let mut warnings = Vec::new();
                let mut chosen = None;
                for arm in arms {
                    let res = eval_condition(&arm.condition, &sc);
                    warnings.extend(res.warnings);
                    if res.matched {
                        chosen = Some(arm);
                        break;
                    }
                }
                state.pc = chosen.map_or(*otherwise, |a| a.target);
                trace.push(TraceEntry::Decision {
                    step: step.clone(),
                    branch: chosen.map(|a| a.branch.clone()),
                    warnings,
                });
            }
            Instr::Action { step } => {
                let s = step_of(plan, step)?;
                let StepKind::Action(action) = &s.kind else {
                    return Err(EngineError::UnknownStep(step.clone()));
                };
                match render_value(&action.input, &scope(facts, &state)) {
                    Ok(input) => {
                        let attempt = state.attempts.get(step).copied().unwrap_or(0) + 1;
                        let directive = Directive::RunAction {
                            step: step.clone(),
                            attempt,
                            input,
                        };
                        return Ok(Advance {
                            state,
                            directive,
                            trace,
                        });
                    }
                    Err(e) => {
                        if let Some(outcome) = step_failed(
                            s,
                            &mut state,
                            &mut trace,
                            error_codes::MAPPING_ERROR,
                            &e.to_string(),
                        ) {
                            return Ok(finish(state, trace, outcome));
                        }
                    }
                }
            }
            Instr::SetVar { step } => {
                if let Some(outcome) = set_variables(plan, step, &mut state, facts, &mut trace)? {
                    return Ok(finish(state, trace, outcome));
                }
            }
            Instr::Delay { step } => {
                if let Some(directive) = delay(plan, step, &mut state, facts, &mut trace)? {
                    return Ok(match directive {
                        DelayResult::Sleep(until) => Advance {
                            state,
                            directive: Directive::Sleep {
                                step: step.clone(),
                                until,
                            },
                            trace,
                        },
                        DelayResult::Finish(outcome) => finish(state, trace, outcome),
                    });
                }
            }
            Instr::Stop { step } => {
                let s = step_of(plan, step)?;
                let StepKind::Stop(stop) = &s.kind else {
                    return Err(EngineError::UnknownStep(step.clone()));
                };
                trace.push(TraceEntry::Stopped {
                    step: step.clone(),
                    status: stop.status,
                    message: stop.message.clone(),
                });
                let outcome = match stop.status {
                    StopStatus::Succeeded => Outcome::Succeeded,
                    StopStatus::Failed => failed(
                        error_codes::STOPPED_BY_FLOW,
                        stop.message
                            .clone()
                            .unwrap_or_else(|| "Stopped by the flow".to_owned()),
                        Some(step),
                    ),
                };
                return Ok(finish(state, trace, outcome));
            }
        }
    }
}

/// Handles a set-variable instruction. Returns the outcome when the execution must fail.
fn set_variables(
    plan: &Plan,
    key: &str,
    state: &mut RunState,
    facts: &Facts<'_>,
    trace: &mut Vec<TraceEntry>,
) -> Result<Option<Outcome>, EngineError> {
    let s = step_of(plan, key)?;
    let StepKind::SetVariable(sv) = &s.kind else {
        return Err(EngineError::UnknownStep(key.to_owned()));
    };
    let mut values = Map::new();
    for a in &sv.assignments {
        // Later assignments can read earlier ones from the same step.
        let mut sc = scope(facts, state);
        if let Some(vars) = sc.get_mut("vars").and_then(Value::as_object_mut) {
            vars.extend(values.clone());
        }
        match render_value(&a.value, &sc) {
            Ok(v) => {
                values.insert(a.name.clone(), v);
            }
            Err(e) => {
                return Ok(step_failed(
                    s,
                    state,
                    trace,
                    error_codes::MAPPING_ERROR,
                    &e.to_string(),
                ));
            }
        }
    }
    state.vars.extend(values.clone());
    trace.push(TraceEntry::VariablesSet {
        step: key.to_owned(),
        values,
    });
    state.pc += 1;
    Ok(None)
}

enum DelayResult {
    Sleep(DateTime<Utc>),
    Finish(Outcome),
}

/// Handles a delay instruction. `Ok(None)` means "continue with the next instruction".
fn delay(
    plan: &Plan,
    key: &str,
    state: &mut RunState,
    facts: &Facts<'_>,
    trace: &mut Vec<TraceEntry>,
) -> Result<Option<DelayResult>, EngineError> {
    if let Some(wait) = state.wait.clone().filter(|w| w.step == key) {
        if facts.now >= wait.until {
            state.wait = None;
            state.pc += 1;
            trace.push(TraceEntry::DelayCompleted {
                step: key.to_owned(),
            });
            return Ok(None);
        }
        return Ok(Some(DelayResult::Sleep(wait.until)));
    }
    let s = step_of(plan, key)?;
    let StepKind::Delay(d) = &s.kind else {
        return Err(EngineError::UnknownStep(key.to_owned()));
    };
    let until = match (d.seconds, d.until.as_deref()) {
        (Some(secs), _) => i64::try_from(secs)
            .ok()
            .and_then(Duration::try_seconds)
            .and_then(|delta| facts.now.checked_add_signed(delta))
            .ok_or_else(|| "delay is out of range".to_owned()),
        (None, Some(tpl)) => render_text(tpl, &scope(facts, state))
            .map_err(|e| e.to_string())
            .and_then(|text| {
                parse_datetime(&text).ok_or_else(|| format!("`{text}` is not a date-time"))
            }),
        (None, None) => Err("delay has neither a duration nor a date-time".to_owned()),
    };
    match until {
        Ok(until) if until <= facts.now => {
            state.pc += 1;
            trace.push(TraceEntry::DelayCompleted {
                step: key.to_owned(),
            });
            Ok(None)
        }
        Ok(until) => {
            state.wait = Some(Wait {
                step: key.to_owned(),
                until,
            });
            trace.push(TraceEntry::DelayScheduled {
                step: key.to_owned(),
                until,
            });
            Ok(Some(DelayResult::Sleep(until)))
        }
        Err(message) => Ok(step_failed(
            s,
            state,
            trace,
            error_codes::DELAY_INVALID_DATETIME,
            &message,
        )
        .map(DelayResult::Finish)),
    }
}

/// Records a non-action step failure; returns the outcome when the execution must fail.
fn step_failed(
    step: &Step,
    state: &mut RunState,
    trace: &mut Vec<TraceEntry>,
    code: &str,
    message: &str,
) -> Option<Outcome> {
    let continued = step.on_error == OnError::Continue;
    trace.push(TraceEntry::StepFailed {
        step: step.key.clone(),
        code: code.to_owned(),
        message: message.to_owned(),
        continued,
    });
    if continued {
        state.pc += 1;
        None
    } else {
        Some(failed(code, message, Some(&step.key)))
    }
}

/// Applies the result of a `RunAction`, deciding between continue, retry and failure. Pure.
pub fn record_result(
    plan: &Plan,
    mut state: RunState,
    step: &str,
    result: &StepResult,
    now: DateTime<Utc>,
) -> Result<(RunState, AfterResult), EngineError> {
    match plan.instrs.get(state.pc) {
        Some(Instr::Action { step: at }) if at == step => {}
        Some(other) => {
            return Err(EngineError::NotAtAction {
                expected: step.to_owned(),
                found: format!("{other:?}"),
            });
        }
        None => return Err(EngineError::PcOutOfRange(state.pc)),
    }
    let s = step_of(plan, step)?;
    match result {
        StepResult::Succeeded(_) => {
            state.attempts.remove(step);
            state.pc += 1;
            Ok((state, AfterResult::Continue))
        }
        StepResult::Failed(err) => {
            let attempts = state.attempts.entry(step.to_owned()).or_insert(0);
            *attempts = attempts.saturating_add(1);
            let attempts = *attempts;
            let max = s
                .retry
                .map_or(DEFAULT_MAX_ATTEMPTS, |r| r.max_attempts.max(1));
            if err.retryable && attempts < max {
                return Ok((
                    state,
                    AfterResult::RetryAt(now + backoff(attempts, err.retry_after_secs)),
                ));
            }
            if s.on_error == OnError::Continue {
                state.attempts.remove(step);
                state.pc += 1;
                return Ok((state, AfterResult::Continue));
            }
            Ok((state, AfterResult::FailExecution(err.clone())))
        }
    }
}

/// Delay before retry number `attempts` (1-based), honouring `Retry-After`, capped at 6 hours.
#[must_use]
pub fn backoff(attempts: u16, retry_after_secs: Option<u64>) -> Duration {
    let idx = usize::from(attempts.saturating_sub(1)).min(BACKOFF_SECS.len() - 1);
    let base = BACKOFF_SECS[idx];
    let hinted = retry_after_secs
        .and_then(|s| i64::try_from(s).ok())
        .unwrap_or(0);
    Duration::seconds(base.max(hinted).min(21_600))
}

/// The output stored for a step that failed but was continued past (`steps.<key>.error`).
#[must_use]
pub fn continued_error_output(err: &StepError) -> Value {
    json!({"error": {"code": err.code, "message": err.message}})
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use serde_json::json;
    use weft_domain::flow::FlowDefinition;

    use super::*;
    use crate::plan::compile;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
            .single()
            .unwrap_or_default()
    }

    fn plan(steps: Value) -> Plan {
        let def: FlowDefinition =
            serde_json::from_value(json!({"trigger": {"type": "manual"}, "steps": steps})).unwrap();
        compile(&def).unwrap()
    }

    fn facts<'a>(
        trigger: &'a Value,
        outputs: &'a Map<String, Value>,
        meta: &'a Value,
        now: DateTime<Utc>,
    ) -> Facts<'a> {
        Facts {
            trigger,
            outputs,
            meta,
            now,
        }
    }

    #[test]
    fn decision_takes_matching_branch_and_renders_action_input() {
        let p = plan(json!([
            {"key": "d", "type": "decision", "branches": [
                {"key": "big", "condition": {"left": "{{ trigger.amount }}", "op": "gt", "right": 100},
                 "steps": [{"key": "a", "type": "action", "connector": "c", "action": "x", "input": {"n": "{{ trigger.amount }}"}}]}
            ], "otherwise": [{"key": "b", "type": "action", "connector": "c", "action": "x"}]}
        ]));
        let trig = json!({"amount": 250});
        let (outs, meta) = (Map::new(), json!({}));
        let adv = advance(&p, RunState::default(), &facts(&trig, &outs, &meta, now())).unwrap();
        assert_eq!(
            adv.directive,
            Directive::RunAction {
                step: "a".into(),
                attempt: 1,
                input: json!({"n": 250})
            }
        );
        assert!(
            matches!(&adv.trace[0], TraceEntry::Decision { branch: Some(b), .. } if b == "big")
        );
    }

    #[test]
    fn retryable_failures_back_off_then_fail() {
        let p = plan(
            json!([{"key": "a", "type": "action", "connector": "c", "action": "x", "retry": {"max_attempts": 2}}]),
        );
        let err = StepResult::Failed(StepError {
            code: "app.unavailable".into(),
            message: "502".into(),
            retryable: true,
            retry_after_secs: None,
            details: Value::Null,
        });
        let (state, after) = record_result(&p, RunState::default(), "a", &err, now()).unwrap();
        assert_eq!(after, AfterResult::RetryAt(now() + Duration::seconds(60)));
        let (_, after) = record_result(&p, state, "a", &err, now()).unwrap();
        assert!(matches!(after, AfterResult::FailExecution(_)));
    }

    #[test]
    fn on_error_continue_moves_on() {
        let p = plan(json!([
            {"key": "a", "type": "action", "connector": "c", "action": "x", "on_error": "continue", "retry": {"max_attempts": 1}},
            {"key": "b", "type": "stop"}
        ]));
        let err = StepResult::Failed(StepError {
            code: "app.invalid_input".into(),
            message: "bad".into(),
            retryable: false,
            retry_after_secs: None,
            details: Value::Null,
        });
        let (state, after) = record_result(&p, RunState::default(), "a", &err, now()).unwrap();
        assert_eq!((after, state.pc), (AfterResult::Continue, 1));
    }

    #[test]
    fn delay_sleeps_then_resumes() {
        let p = plan(
            json!([{"key": "w", "type": "delay", "seconds": 3600}, {"key": "v", "type": "set_variable", "assignments": [{"name": "x", "value": 1}]}]),
        );
        let (trig, outs, meta) = (json!({}), Map::new(), json!({}));
        let adv = advance(&p, RunState::default(), &facts(&trig, &outs, &meta, now())).unwrap();
        let until = now() + Duration::seconds(3600);
        assert_eq!(
            adv.directive,
            Directive::Sleep {
                step: "w".into(),
                until
            }
        );
        // Woken early: sleeps again until the same time.
        let early = advance(
            &p,
            adv.state.clone(),
            &facts(&trig, &outs, &meta, now() + Duration::seconds(10)),
        )
        .unwrap();
        assert_eq!(
            early.directive,
            Directive::Sleep {
                step: "w".into(),
                until
            }
        );
        let done = advance(&p, adv.state, &facts(&trig, &outs, &meta, until)).unwrap();
        assert_eq!(done.directive, Directive::Finish(Outcome::Succeeded));
        assert_eq!(done.state.vars.get("x"), Some(&json!(1)));
    }

    #[test]
    fn delay_until_in_the_past_continues_and_bad_dates_fail() {
        let p = plan(json!([{"key": "w", "type": "delay", "until": "{{ trigger.at }}"}]));
        let (outs, meta) = (Map::new(), json!({}));
        let past = json!({"at": "2020-01-01T00:00:00Z"});
        assert_eq!(
            advance(&p, RunState::default(), &facts(&past, &outs, &meta, now()))
                .unwrap()
                .directive,
            Directive::Finish(Outcome::Succeeded)
        );
        let bad = json!({"at": "soon"});
        let adv = advance(&p, RunState::default(), &facts(&bad, &outs, &meta, now())).unwrap();
        assert!(
            matches!(adv.directive, Directive::Finish(Outcome::Failed { ref code, .. }) if code == error_codes::DELAY_INVALID_DATETIME)
        );
    }

    #[test]
    fn stop_failed_fails_the_execution() {
        let p = plan(json!([{"key": "s", "type": "stop", "status": "failed", "message": "nope"}]));
        let (trig, outs, meta) = (json!({}), Map::new(), json!({}));
        let adv = advance(&p, RunState::default(), &facts(&trig, &outs, &meta, now())).unwrap();
        assert!(
            matches!(adv.directive, Directive::Finish(Outcome::Failed { ref message, .. }) if message == "nope")
        );
    }

    #[test]
    fn backoff_schedule_and_retry_after() {
        assert_eq!(backoff(1, None), Duration::seconds(60));
        assert_eq!(backoff(8, None), Duration::seconds(21_600));
        assert_eq!(backoff(30, None), Duration::seconds(21_600));
        assert_eq!(backoff(1, Some(600)), Duration::seconds(600));
        assert_eq!(backoff(1, Some(99_999)), Duration::seconds(21_600));
    }
}
