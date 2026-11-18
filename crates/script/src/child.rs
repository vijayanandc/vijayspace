//! The protocol for running one script in a child process (doc 08 §7): the parent writes a
//! [`ChildRequest`] as JSON to the child's stdin and reads a [`ChildResponse`] from its stdout.
//! The child runs under OS memory and CPU limits, so a script that defeats the in-engine size
//! accounting can only kill its own process.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Limits, ScriptError, run};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChildRequest {
    pub code: String,
    pub input: Value,
    pub context: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ChildResponse {
    Ok {
        value: Value,
        logs: Vec<String>,
    },
    Error {
        limit: bool,
        message: String,
        line: Option<usize>,
        column: Option<usize>,
    },
}

impl From<ChildResponse> for Result<crate::ScriptOutput, ScriptError> {
    fn from(r: ChildResponse) -> Self {
        match r {
            ChildResponse::Ok { value, logs } => Ok(crate::ScriptOutput { value, logs }),
            ChildResponse::Error {
                limit: true,
                message,
                line,
                column,
            } => Err(ScriptError::LimitExceeded {
                message,
                line,
                column,
            }),
            ChildResponse::Error {
                limit: false,
                message,
                line,
                column,
            } => Err(ScriptError::Error {
                message,
                line,
                column,
            }),
        }
    }
}

/// Runs the request in this process and builds the response (what the child does).
#[must_use]
pub fn respond(request_json: &str, limits: &Limits) -> ChildResponse {
    let request: ChildRequest = match serde_json::from_str(request_json) {
        Ok(r) => r,
        Err(e) => {
            return ChildResponse::Error {
                limit: false,
                message: format!("bad script request: {e}"),
                line: None,
                column: None,
            };
        }
    };
    match run(&request.code, &request.input, &request.context, limits) {
        Ok(out) => ChildResponse::Ok {
            value: out.value,
            logs: out.logs,
        },
        Err(e) => {
            let (line, column) = e.position();
            ChildResponse::Error {
                limit: matches!(e, ScriptError::LimitExceeded { .. }),
                // The message without the position prefix; the parent re-adds it.
                message: match e {
                    ScriptError::Error { message, .. }
                    | ScriptError::LimitExceeded { message, .. } => message,
                },
                line,
                column,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use serde_json::json;

    use super::*;

    #[test]
    fn requests_and_responses_round_trip() {
        let req = serde_json::to_string(&ChildRequest {
            code: "#{ n: input.n }".into(),
            input: json!({"n": 2}),
            context: json!({}),
        })
        .unwrap();
        let ok = respond(&req, &Limits::default());
        let back: ChildResponse =
            serde_json::from_str(&serde_json::to_string(&ok).unwrap()).unwrap();
        assert_eq!(Result::from(back).unwrap().value, json!({"n": 2}));
        let req = serde_json::to_string(&ChildRequest {
            code: "1;\nthrow \"x\"".into(),
            input: json!({}),
            context: json!({}),
        })
        .unwrap();
        let err: Result<crate::ScriptOutput, ScriptError> =
            respond(&req, &Limits::default()).into();
        let err = err.unwrap_err();
        assert_eq!(err.code(), "script.error");
        assert_eq!(err.position().0, Some(2));
        assert!(matches!(
            respond("not json", &Limits::default()),
            ChildResponse::Error { limit: false, .. }
        ));
    }
}
