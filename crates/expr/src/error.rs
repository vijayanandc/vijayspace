//! Errors produced while compiling or evaluating templates.

/// A template could not be parsed or evaluated.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ExprError {
    /// The template text is not valid syntax.
    #[error("template syntax error in `{source_text}`: {message}")]
    Syntax {
        source_text: String,
        message: String,
    },
    /// The template is valid but evaluating it failed (e.g. fuel exhausted, bad filter argument).
    #[error("template evaluation failed for `{source_text}`: {message}")]
    Eval {
        source_text: String,
        message: String,
    },
}

impl ExprError {
    pub(crate) fn syntax(source_text: &str, err: &minijinja::Error) -> Self {
        Self::Syntax {
            source_text: clip(source_text),
            message: err.to_string(),
        }
    }

    pub(crate) fn eval(source_text: &str, err: &impl ToString) -> Self {
        Self::Eval {
            source_text: clip(source_text),
            message: err.to_string(),
        }
    }
}

fn clip(s: &str) -> String {
    s.chars().take(200).collect()
}
