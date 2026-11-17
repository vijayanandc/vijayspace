//! weft-engine: compiles a `FlowDefinition` into a Plan (IR) and interprets it purely.
//!
//! Changes here need human review (🔒). Invariants are property-tested in `tests/props.rs`.

mod interpreter;
mod plan;
#[cfg(any(test, feature = "testing"))]
pub mod simulator;

pub use interpreter::{
    Advance, AfterResult, BACKOFF_SECS, DEFAULT_MAX_ATTEMPTS, Directive, EngineError, Facts,
    MAX_INSTRUCTIONS, Outcome, RunState, StepError, StepResult, TraceEntry, Wait, advance, backoff,
    continued_error_output, record_result,
};
pub use plan::{Arm, CompileError, Instr, Pc, Plan, compile};
