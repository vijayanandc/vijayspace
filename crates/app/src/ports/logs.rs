//! Ports: the detailed log of a run (the HTTP calls its steps made, and events that leave no step row).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;
use weft_domain::ids::{ExecutionId, OrgId};

use super::Res;

/// How serious a log line is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

/// What a log line is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogKind {
    /// The run itself (queued, started, finished, waiting).
    Run,
    /// A step run (started, finished, decision taken, …).
    Step,
    /// One HTTP call to an app.
    Http,
    /// A request held back by the connection's limits.
    Limit,
    /// An attempt failed and a retry is scheduled.
    Retry,
    /// Anything else worth knowing (e.g. a worker stopped mid-step).
    Event,
}

impl LogLevel {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }

    /// Reads a stored level (unknown text reads as `info`).
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s {
            "warn" => Self::Warn,
            "error" => Self::Error,
            _ => Self::Info,
        }
    }
}

impl LogKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Run => "run",
            Self::Step => "step",
            Self::Http => "http",
            Self::Limit => "limit",
            Self::Retry => "retry",
            Self::Event => "event",
        }
    }

    /// Reads a stored kind (unknown text reads as `event`).
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s {
            "run" => Self::Run,
            "step" => Self::Step,
            "http" => Self::Http,
            "limit" => Self::Limit,
            "retry" => Self::Retry,
            _ => Self::Event,
        }
    }
}

/// One stored log line of a run.
#[derive(Debug, Clone, PartialEq)]
pub struct LogRecord {
    pub id: Uuid,
    pub execution_id: ExecutionId,
    pub step_key: Option<String>,
    pub attempt: Option<u16>,
    pub level: LogLevel,
    pub kind: LogKind,
    pub message: String,
    pub details: Option<Value>,
    pub at: DateTime<Utc>,
}

#[async_trait]
pub trait LogRepo: Send + Sync {
    /// Appends lines to one run's log (they take the run's day partition). Lines for a run that no longer
    /// exists in `org` are dropped.
    async fn append_logs(
        &self,
        org: OrgId,
        execution: ExecutionId,
        records: &[LogRecord],
    ) -> Res<()>;
    /// A run's newest `limit` stored lines, oldest first.
    async fn list_logs(
        &self,
        org: OrgId,
        execution: ExecutionId,
        limit: u32,
    ) -> Res<Vec<LogRecord>>;
}
