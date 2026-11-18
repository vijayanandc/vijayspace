//! Script failures: configuration errors that are never retried automatically.

use rhai::{EvalAltResult, ParseError, Position};

/// Why a script failed. `line`/`column` are 1-based when known.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScriptError {
    /// Syntax error, `throw`, or a runtime error (wrong types, unknown function, helper error).
    #[error("{}", located(.message, *.line, *.column))]
    Error {
        message: String,
        line: Option<usize>,
        column: Option<usize>,
    },
    /// An operation, depth, size or time limit was hit.
    #[error("{}", located(.message, *.line, *.column))]
    LimitExceeded {
        message: String,
        line: Option<usize>,
        column: Option<usize>,
    },
}

/// Longest error message kept (a thrown value can be arbitrarily large).
const MAX_MESSAGE_CHARS: usize = 2_000;

fn bounded(message: String) -> String {
    if message.chars().count() <= MAX_MESSAGE_CHARS {
        return message;
    }
    let mut cut: String = message.chars().take(MAX_MESSAGE_CHARS).collect();
    cut.push('…');
    cut
}

fn located(message: &str, line: Option<usize>, column: Option<usize>) -> String {
    match (line, column) {
        (Some(l), Some(c)) => format!("line {l}, column {c}: {message}"),
        (Some(l), None) => format!("line {l}: {message}"),
        (None, Some(_) | None) => message.to_owned(),
    }
}

impl ScriptError {
    /// Stable error code (`script.error` or `script.limit_exceeded`).
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Error { .. } => "script.error",
            Self::LimitExceeded { .. } => "script.limit_exceeded",
        }
    }

    /// `(line, column)` of the problem, when known.
    #[must_use]
    pub const fn position(&self) -> (Option<usize>, Option<usize>) {
        match self {
            Self::Error { line, column, .. } | Self::LimitExceeded { line, column, .. } => {
                (*line, *column)
            }
        }
    }

    pub(crate) fn limit(message: impl Into<String>) -> Self {
        Self::LimitExceeded {
            message: message.into(),
            line: None,
            column: None,
        }
    }

    pub(crate) fn plain(message: impl Into<String>) -> Self {
        Self::Error {
            message: message.into(),
            line: None,
            column: None,
        }
    }
}

/// Rhai appends `(line N, position M)` to many messages; the position is reported separately.
fn without_position(message: &str) -> String {
    match message.rfind(" (line ") {
        Some(i) if message.ends_with(')') => message[..i].to_owned(),
        Some(_) | None => message.to_owned(),
    }
}

fn pos(p: Position) -> (Option<usize>, Option<usize>) {
    (p.line(), p.position())
}

impl From<ParseError> for ScriptError {
    fn from(e: ParseError) -> Self {
        let (line, column) = pos(e.position());
        let message = bounded(e.err_type().to_string());
        // Depth limits surface as parse errors; they are limits, not syntax mistakes.
        if matches!(e.err_type(), rhai::ParseErrorType::ExprTooDeep) {
            return Self::LimitExceeded {
                message,
                line,
                column,
            };
        }
        Self::Error {
            message,
            line,
            column,
        }
    }
}

/// An error raised inside a script function is reported as the inner error, with the innermost
/// known position and the outermost named function, i.e. the script's own `fn` rather than a
/// built-in such as `map` it passed a closure to (closures have no useful name).
fn innermost(e: Box<EvalAltResult>) -> (Box<EvalAltResult>, Option<String>, Position) {
    let mut position = e.position();
    let mut function = None;
    let mut e = e;
    while matches!(*e, EvalAltResult::ErrorInFunctionCall(..)) {
        if let EvalAltResult::ErrorInFunctionCall(name, _, inner, _) = *e {
            if function.is_none() && !name.starts_with("anon$") {
                function = Some(name);
            }
            if !inner.position().is_none() {
                position = inner.position();
            }
            e = inner;
        }
    }
    (e, function, position)
}

/// The message for an evaluation error, with a hint where Rhai's own wording leaves users stuck.
fn eval_message(e: &EvalAltResult, in_function: bool) -> String {
    if let EvalAltResult::ErrorRuntime(value, _) = e {
        // `throw "message"`: the thrown value is the message.
        return value.to_string();
    }
    if matches!(e, EvalAltResult::ErrorTerminated(..)) {
        return "the script ran for too long".to_owned();
    }
    let message = without_position(&e.to_string());
    if let EvalAltResult::ErrorVariableNotFound(name, _) = e
        && in_function
        && (name == "input" || name == "context")
    {
        return format!(
            "{message}. Functions only see their parameters: pass it in, e.g. `fn total({name}) {{ … }}` and call `total({name})`"
        );
    }
    if matches!(e, EvalAltResult::ErrorDataRace(..)) {
        return format!(
            "{message}: a closure reads a value that the function it was passed to is working on. Copy what it needs first, e.g. `let factor = input.factor;`, and use that in the closure"
        );
    }
    message
}

impl From<Box<EvalAltResult>> for ScriptError {
    fn from(e: Box<EvalAltResult>) -> Self {
        let (e, function, position) = innermost(e);
        let (line, column) = pos(position);
        let limit = matches!(
            *e,
            EvalAltResult::ErrorTooManyOperations(..)
                | EvalAltResult::ErrorDataTooLarge(..)
                | EvalAltResult::ErrorStackOverflow(..)
                | EvalAltResult::ErrorTerminated(..)
        );
        let mut message = eval_message(&e, function.is_some());
        if let Some(name) = function {
            message = format!("{message} (in function `{name}`)");
        }
        let message = bounded(message);
        if limit {
            Self::LimitExceeded {
                message,
                line,
                column,
            }
        } else {
            Self::Error {
                message,
                line,
                column,
            }
        }
    }
}
