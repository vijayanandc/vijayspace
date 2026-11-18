//! `ScriptRunner`s (doc 08 §7):
//! - [`ProcessScriptRunner`] (production): each script runs in a child process under OS memory
//!   and CPU limits, so a script that defeats the engine's size accounting only kills itself.
//! - [`PooledScriptRunner`] (tests, development): a thread per script in this process, at most
//!   `concurrency` at once (each holds its permit until it finishes).

use std::{path::PathBuf, process::Stdio, sync::Arc, time::Duration};

use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::sync::Semaphore;
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    process::Command,
};
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
        let permit = Arc::clone(&self.permits)
            .acquire_owned()
            .await
            .map_err(|_| ConnectorError::transient("the script runner is shutting down"))?;
        let (code, limits) = (code.to_owned(), self.limits.clone());
        // Its own thread with a large stack (the blocking pool's 2 MB is too small for deeply
        // nested values). The thread holds the permit, so even if this future is dropped (a step
        // timeout) no more than `concurrency` scripts run at once.
        let (done, result) = tokio::sync::oneshot::channel();
        std::thread::Builder::new()
            .name("weft-script".into())
            .stack_size(weft_script::STACK_BYTES)
            .spawn(move || {
                let _permit = permit;
                let _ = done.send(weft_script::run(&code, &input, &context, &limits));
            })
            .map_err(|e| ConnectorError::transient(format!("starting a script thread: {e}")))?;
        match result.await {
            Ok(Ok(out)) => Ok(ScriptRun {
                output: out.value,
                logs: out.logs,
            }),
            Ok(Err(e)) => Err(script_error(&e)),
            Err(_) => Err(crashed("the script crashed".to_owned())),
        }
    }
}

/// The most a child may write: the 1 MB result, the logs and JSON overhead.
const MAX_STDOUT: u64 = 2 << 20;

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

fn crashed(message: String) -> ConnectorError {
    ConnectorError::Coded {
        code: "script.error",
        message,
        details: Value::Null,
    }
}

/// Signals that stop a child from outside (a shutdown or Ctrl-C), not because of the script.
#[cfg(unix)]
const OUTSIDE_SIGNALS: [i32; 3] = [1, 2, 15]; // SIGHUP, SIGINT, SIGTERM

/// A child that exits without a response. Stopped by a shutdown signal: try again later. Killed by
/// any other signal: the OS memory or CPU cap (or a stack overflow) stopped it, which is a limit.
/// Any other exit is an engine failure.
fn child_failed(status: std::process::ExitStatus) -> ConnectorError {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt as _;
        if let Some(signal) = status.signal() {
            if OUTSIDE_SIGNALS.contains(&signal) {
                return ConnectorError::transient("the script runner was stopped");
            }
            return resources_exceeded();
        }
    }
    crashed(format!("the script runner stopped unexpectedly ({status})"))
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
            // Nothing of the server's environment (master keys, database URL, app secrets) reaches user code.
            .env_clear()
            .env("WEFT_SCRIPT_MEMORY_MB", self.memory_mb.to_string())
            // One malloc arena (a second thread would reserve another 64 MB of address space),
            // and no backtrace capture for the panics the sandbox catches.
            .env("MALLOC_ARENA_MAX", "1")
            .env("RUST_BACKTRACE", "0")
            .env("RUST_LIB_BACKTRACE", "0")
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
        let Some(stdout) = child.stdout.take() else {
            return Err(ConnectorError::transient("the script runner has no output"));
        };
        // Read at most MAX_STDOUT (+1 to notice more): a script cannot flood this process.
        let run = async {
            let mut buf = Vec::new();
            stdout.take(MAX_STDOUT + 1).read_to_end(&mut buf).await?;
            if u64::try_from(buf.len()).unwrap_or(u64::MAX) > MAX_STDOUT {
                return Ok::<_, std::io::Error>(None);
            }
            let status = child.wait().await?;
            Ok(Some((buf, status)))
        };
        let (stdout, status) = match tokio::time::timeout(self.wall, run).await {
            Ok(Ok(Some(done))) => done,
            // Too much output or too slow: the child is killed when it is dropped.
            Ok(Ok(None)) | Err(_) => return Err(resources_exceeded()),
            Ok(Err(e)) => {
                return Err(ConnectorError::transient(format!(
                    "running the script: {e}"
                )));
            }
        };
        if !status.success() {
            return Err(child_failed(status));
        }
        let response: ChildResponse = serde_json::from_slice(&stdout)
            .map_err(|_| crashed("the script runner returned no result".to_owned()))?;
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

    #[tokio::test]
    async fn deeply_nested_values_do_not_overflow_the_script_thread() {
        let runner = PooledScriptRunner::new(1, Limits::default());
        let out = runner
            .run(
                "let a = #{}; for i in 0..1000 { a = #{x: a}; } a.len()",
                json!({}),
                json!({}),
            )
            .await
            .unwrap();
        assert_eq!(out.output, json!({"result": 1}));
    }

    #[cfg(unix)]
    #[test]
    fn a_killed_child_hit_a_limit_and_any_other_exit_is_a_script_error() {
        use std::os::unix::process::ExitStatusExt as _;
        let code = |e: ConnectorError| e.code().to_owned();
        // Raw wait statuses: 9 = killed by SIGKILL, 6 = SIGABRT (allocation failure), 256 = exit 1,
        // 15 = SIGTERM (a shutdown, retried later).
        assert_eq!(
            code(child_failed(std::process::ExitStatus::from_raw(9))),
            "script.limit_exceeded"
        );
        assert_eq!(
            code(child_failed(std::process::ExitStatus::from_raw(6))),
            "script.limit_exceeded"
        );
        assert_eq!(
            code(child_failed(std::process::ExitStatus::from_raw(256))),
            "script.error"
        );
        assert_eq!(
            code(child_failed(std::process::ExitStatus::from_raw(15))),
            "app.unavailable"
        );
    }
}
