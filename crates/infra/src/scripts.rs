//! `ScriptRunner` that runs custom functions on blocking threads, at most N at a time (doc 08 §7).
//! This is the one sanctioned use of `spawn_blocking` outside the worker and server.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::sync::Semaphore;
use weft_connector_sdk::{ConnectorError, ScriptRun, ScriptRunner};
use weft_script::{Limits, ScriptError};

/// Runs scripts with the platform limits; `concurrency` bounds how many run at once.
#[derive(Debug)]
pub struct PooledScriptRunner {
    permits: Arc<Semaphore>,
    limits: Limits,
}

impl PooledScriptRunner {
    #[must_use]
    pub fn new(concurrency: usize, limits: Limits) -> Self {
        Self {
            permits: Arc::new(Semaphore::new(concurrency.max(1))),
            limits,
        }
    }
}

/// A script failure as a step error: `script.error` or `script.limit_exceeded`, never retried.
#[must_use]
pub fn script_error(e: &ScriptError) -> ConnectorError {
    let (line, column) = e.position();
    ConnectorError::Coded {
        code: e.code(),
        message: e.to_string(),
        details: json!({"line": line, "column": column}),
    }
}

#[async_trait]
impl ScriptRunner for PooledScriptRunner {
    async fn run(
        &self,
        code: &str,
        input: Value,
        context: Value,
    ) -> Result<ScriptRun, ConnectorError> {
        let _permit = self
            .permits
            .acquire()
            .await
            .map_err(|_| ConnectorError::transient("the script runner is shutting down"))?;
        let (code, limits) = (code.to_owned(), self.limits.clone());
        let joined =
            tokio::task::spawn_blocking(move || weft_script::run(&code, &input, &context, &limits))
                .await;
        match joined {
            Ok(Ok(out)) => Ok(ScriptRun {
                output: out.value,
                logs: out.logs,
            }),
            Ok(Err(e)) => Err(script_error(&e)),
            Err(e) => Err(ConnectorError::permanent(format!(
                "the script crashed: {e}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[tokio::test]
    async fn runs_scripts_and_maps_failures_to_coded_errors() {
        let runner = PooledScriptRunner::new(2, Limits::default());
        let out = runner
            .run(
                "print(\"hi\"); #{ n: input.n + 1 }",
                json!({"n": 1}),
                json!({}),
            )
            .await
            .unwrap();
        assert_eq!(out.output, json!({"n": 2}));
        assert_eq!(out.logs, vec!["hi"]);
        let err = runner
            .run("throw \"nope\"", json!({}), json!({}))
            .await
            .unwrap_err();
        assert_eq!(err.code(), "script.error");
        assert!(!err.retryable());
        let limit = runner
            .run("loop { }", json!({}), json!({}))
            .await
            .unwrap_err();
        assert_eq!(limit.code(), "script.limit_exceeded");
        assert_eq!(limit.details()["line"], 1);
        // The script crate's codes are the platform's stable codes.
        assert_eq!(err.code(), weft_domain::error_codes::SCRIPT_ERROR);
        assert_eq!(
            limit.code(),
            weft_domain::error_codes::SCRIPT_LIMIT_EXCEEDED
        );
    }
}
