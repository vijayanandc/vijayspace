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
        let message = e.err_type().to_string();
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

impl From<Box<EvalAltResult>> for ScriptError {
    fn from(e: Box<EvalAltResult>) -> Self {
        let (line, column) = pos(e.position());
        let limit = matches!(
            *e,
            EvalAltResult::ErrorTooManyOperations(..)
                | EvalAltResult::ErrorDataTooLarge(..)
                | EvalAltResult::ErrorStackOverflow(..)
                | EvalAltResult::ErrorTerminated(..)
        );
        // `throw "message"`: the thrown value is the message.
        let message = if let EvalAltResult::ErrorRuntime(value, _) = &*e {
            value.to_string()
        } else if matches!(*e, EvalAltResult::ErrorTerminated(..)) {
            "the script ran for too long".to_owned()
        } else {
            without_position(&e.to_string())
        };
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
