//! Building a sandboxed engine per run, running a script, and normalising its output.

use std::{
    panic::AssertUnwindSafe,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use rhai::{Dynamic, Engine, Scope};
use serde_json::{Map, Value};

use crate::{ScriptError, helpers};

/// Sandbox limits (doc 08 §7). The operation budget is the deterministic CPU limit; the wall time
/// is only a safety net.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limits {
    pub max_operations: u64,
    pub max_call_levels: usize,
    pub max_expr_depth: usize,
    pub max_function_expr_depth: usize,
    pub max_string_size: usize,
    pub max_array_size: usize,
    pub max_map_size: usize,
    pub max_source_bytes: usize,
    pub max_output_bytes: usize,
    /// Nesting depth of the result (stored as JSON: MySQL allows 100 levels, `serde_json` 128).
    pub max_output_depth: usize,
    pub max_log_lines: usize,
    pub max_log_bytes: usize,
    pub max_wall_time: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_operations: 1_000_000,
            max_call_levels: 32,
            max_expr_depth: 64,
            max_function_expr_depth: 32,
            max_string_size: 1 << 20,
            max_array_size: 10_000,
            max_map_size: 10_000,
            max_source_bytes: 64 * 1024,
            max_output_bytes: 1 << 20,
            max_output_depth: 64,
            max_log_lines: 100,
            max_log_bytes: 10 * 1024,
            max_wall_time: Duration::from_secs(2),
        }
    }
}

/// A successful run: the step output (always an object) and the captured `print`/`debug` lines.
#[derive(Debug, Clone, PartialEq)]
pub struct ScriptOutput {
    pub value: Value,
    pub logs: Vec<String>,
}

/// Captured log lines, bounded by line count and bytes.
#[derive(Debug, Default)]
struct Log {
    lines: Vec<String>,
    bytes: usize,
    dropped: bool,
}

impl Log {
    fn push(&mut self, line: &str, limits: &Limits) {
        if self.lines.len() >= limits.max_log_lines
            || self.bytes + line.len() > limits.max_log_bytes
        {
            self.dropped = true;
            return;
        }
        self.bytes += line.len();
        self.lines.push(line.to_owned());
    }

    fn finish(self) -> Vec<String> {
        let mut lines = self.lines;
        if self.dropped {
            lines.push("… more log lines were dropped".to_owned());
        }
        lines
    }
}

fn engine(limits: &Limits, log: &Arc<Mutex<Log>>) -> Engine {
    let mut engine = Engine::new();
    engine
        .set_max_operations(limits.max_operations)
        .set_max_call_levels(limits.max_call_levels)
        .set_max_expr_depths(limits.max_expr_depth, limits.max_function_expr_depth)
        .set_max_string_size(limits.max_string_size)
        .set_max_array_size(limits.max_array_size)
        .set_max_map_size(limits.max_map_size)
        .set_strict_variables(true);
    engine.disable_symbol("eval");
    // Curried values escape the size limits: no `Fn("name")` and no currying. (Memory as a whole
    // is capped by the child process's OS limit; this only narrows the in-process runner's risk.)
    engine.disable_symbol("Fn");
    engine.disable_symbol("curry");
    let (print_log, print_limits) = (Arc::clone(log), limits.clone());
    engine.on_print(move |s| {
        if let Ok(mut l) = print_log.lock() {
            l.push(s, &print_limits);
        }
    });
    let (debug_log, debug_limits) = (Arc::clone(log), limits.clone());
    engine.on_debug(move |s, _src, pos| {
        if let Ok(mut l) = debug_log.lock() {
            l.push(&format!("[{pos}] {s}"), &debug_limits);
        }
    });
    // Safety net only: monotonic time, never business logic.
    let started = Instant::now();
    let wall = limits.max_wall_time;
    engine.on_progress(move |_ops| (started.elapsed() > wall).then_some(Dynamic::UNIT));
    helpers::register(&mut engine);
    engine
}

fn check_size(code: &str, limits: &Limits) -> Result<(), ScriptError> {
    if code.len() > limits.max_source_bytes {
        return Err(ScriptError::limit(format!(
            "the script is longer than {} KB",
            limits.max_source_bytes / 1024
        )));
    }
    Ok(())
}

/// The variables every script sees (strict variables: anything else is a compile error). They are
/// ordinary variables, so a script may change its copy (`input.total = …; input`). Functions see
/// only their parameters (Rhai's rule): scripts pass them in, e.g. `fn total(input) {…} total(input)`.
fn scope<'a>(input: Dynamic, context: Dynamic) -> Scope<'a> {
    let mut scope = Scope::new();
    scope.push("input", input);
    scope.push("context", context);
    scope
}

/// Runs `f`, turning a panic inside the engine into a script error. Rhai has a few `unreachable!`
/// paths a script can reach (e.g. assigning to a property of a `const` map); one script must never
/// take down a worker, and its author needs a message rather than a crash.
fn guarded<T>(f: impl FnOnce() -> Result<T, ScriptError>) -> Result<T, ScriptError> {
    std::panic::catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|payload| {
        let detail = payload
            .downcast_ref::<&str>()
            .map(|s| (*s).to_owned())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_default();
        Err(ScriptError::plain(format!(
            "the script engine cannot run this script ({detail}). Changing a `const` value is a common cause: use `let` instead"
        )))
    })
}

/// Compiles a script without running it (publish-time validation).
pub fn check(code: &str, limits: &Limits) -> Result<(), ScriptError> {
    check_size(code, limits)?;
    let log = Arc::new(Mutex::new(Log::default()));
    let empty = || Dynamic::from_map(rhai::Map::new());
    let engine = engine(limits, &log);
    guarded(|| Ok(engine.compile_with_scope(&scope(empty(), empty()), code)?))?;
    Ok(())
}

/// Runs a script with `input` and `context` in scope. A map result becomes the output object;
/// anything else becomes `{"result": value}`.
pub fn run(
    code: &str,
    input: &Value,
    context: &Value,
    limits: &Limits,
) -> Result<ScriptOutput, ScriptError> {
    check_size(code, limits)?;
    let log = Arc::new(Mutex::new(Log::default()));
    let engine = engine(limits, &log);
    let mut scope = scope(to_dynamic(input)?, to_dynamic(context)?);
    let result: Dynamic = guarded(|| {
        let ast = engine.compile_with_scope(&scope, code)?;
        Ok(engine.eval_ast_with_scope(&mut scope, &ast)?)
    })?;
    let value = output(&result)?;
    if depth(&value) > limits.max_output_depth {
        return Err(ScriptError::limit(format!(
            "the result is nested more than {} levels deep",
            limits.max_output_depth
        )));
    }
    let size = value.to_string().len();
    if size > limits.max_output_bytes {
        return Err(ScriptError::limit(format!(
            "the result is larger than {} KB",
            limits.max_output_bytes / 1024
        )));
    }
    drop(engine); // releases the closures' clones of the log
    let logs = Arc::try_unwrap(log)
        .ok()
        .and_then(|m| m.into_inner().ok())
        .map(Log::finish)
        .unwrap_or_default();
    Ok(ScriptOutput { value, logs })
}

/// Nesting depth of a JSON value (a scalar is 0), without recursion.
fn depth(value: &Value) -> usize {
    let mut deepest = 0;
    let mut stack = vec![(value, 0_usize)];
    while let Some((v, d)) = stack.pop() {
        deepest = deepest.max(d);
        if let Value::Array(items) = v {
            stack.extend(items.iter().map(|i| (i, d + 1)));
        } else if let Value::Object(fields) = v {
            stack.extend(fields.values().map(|i| (i, d + 1)));
        }
    }
    deepest
}

pub(crate) fn to_dynamic(value: &Value) -> Result<Dynamic, ScriptError> {
    rhai::serde::to_dynamic(value).map_err(|e| ScriptError::plain(format!("input: {e}")))
}

pub(crate) fn to_json(value: &Dynamic) -> Result<Value, ScriptError> {
    rhai::serde::from_dynamic::<Value>(value)
        .map_err(|e| ScriptError::plain(format!("the result cannot be turned into JSON: {e}")))
}

fn output(result: &Dynamic) -> Result<Value, ScriptError> {
    if result.is_unit() {
        return Ok(Value::Object(Map::from_iter([(
            "result".to_owned(),
            Value::Null,
        )])));
    }
    let value = to_json(result)?;
    if value.is_object() {
        return Ok(value);
    }
    Ok(Value::Object(Map::from_iter([(
        "result".to_owned(),
        value,
    )])))
}
