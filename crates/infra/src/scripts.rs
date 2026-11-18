//! `ScriptRunner`s (doc 08 §7):
//! - [`ProcessScriptRunner`] (production): each script runs in a child process under OS memory
//!   and CPU limits, so a script that defeats the engine's size accounting only kills itself.
//! - [`PooledScriptRunner`] (tests, development): blocking threads in this process. This is the
//!   one sanctioned use of `spawn_blocking` outside the worker and server.

use std::{path::PathBuf, process::Stdio, sync::Arc, time::Duration};

use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::sync::Semaphore;
use tokio::{io::AsyncWriteExt as _, process::Command};
use weft_connector_sdk::{ConnectorError, ScriptRun, ScriptRunner};
use weft_script::{
    Limits, ScriptError,
    child::{ChildRequest, ChildResponse},
};

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

/// The subcommand of the `weft` binary that runs one script (see `weft_script::child`).
pub const CHILD_SUBCOMMAND: &str = "script-child";

/// Runs each script in a child process of `exe` (the `weft` binary), at most N at a time.
#[derive(Debug)]
pub struct ProcessScriptRunner {
    exe: PathBuf,
    permits: Arc<Semaphore>,
    /// Address-space cap of the child, in MB (`RLIMIT_AS`).
    memory_mb: u64,
    /// The child is killed after this long (the engine's own guards normally stop it first).
    wall: Duration,
}

impl ProcessScriptRunner {
    #[must_use]
    pub fn new(exe: PathBuf, concurrency: usize, memory_mb: u64, wall: Duration) -> Self {
        Self {
            exe,
            permits: Arc::new(Semaphore::new(concurrency.max(1))),
            memory_mb,
            wall,
        }
    }
}

fn resources_exceeded() -> ConnectorError {
    ConnectorError::Coded {
        code: "script.limit_exceeded",
        message: "the script used too much memory or time".into(),
        details: Value::Null,
    }
}

#[async_trait]
impl ScriptRunner for ProcessScriptRunner {
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
        let request = serde_json::to_vec(&ChildRequest {
            code: code.to_owned(),
            input,
            context,
        })
        .map_err(|e| ConnectorError::permanent(format!("encoding the script request: {e}")))?;
        let mut child = Command::new(&self.exe)
            .arg(CHILD_SUBCOMMAND)
            .env("WEFT_SCRIPT_MEMORY_MB", self.memory_mb.to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| ConnectorError::transient(format!("starting the script runner: {e}")))?;
        if let Some(mut stdin) = child.stdin.take() {
            // A child that dies early closes the pipe; its exit status tells why.
            let _ = stdin.write_all(&request).await;
        }
        let output = match tokio::time::timeout(self.wall, child.wait_with_output()).await {
            Ok(Ok(output)) => output,
            Ok(Err(e)) => {
                return Err(ConnectorError::transient(format!(
                    "running the script: {e}"
                )));
            }
            Err(_) => return Err(resources_exceeded()), // dropped: the child is killed
        };
        if !output.status.success() {
            return Err(resources_exceeded());
        }
        let response: ChildResponse =
            serde_json::from_slice(&output.stdout).map_err(|_| resources_exceeded())?;
        let result: Result<weft_script::ScriptOutput, ScriptError> = response.into();
        match result {
            Ok(out) => Ok(ScriptRun {
                output: out.value,
                logs: out.logs,
            }),
            Err(e) => Err(script_error(&e)),
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
