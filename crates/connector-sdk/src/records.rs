//! Record-oriented capability for metadata-driven apps (Zoho CRM, Appblocks).
//! See `docs/plan/13-mvp-zoho-crm-appblocks.md` §4.1.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{context::ActionCtx, error::ConnectorError, manifest::FieldSpec};

/// A module / object type in the app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectInfo {
    pub key: String,
    pub label: String,
}

/// One record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Record {
    pub id: String,
    #[serde(default)]
    pub modified_at: Option<String>,
    pub values: Map<String, Value>,
}

/// Identity of a written record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordRef {
    pub id: String,
}

/// A page of records plus the cursor for the next page (`None` = done).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Page {
    pub records: Vec<Record>,
    pub next_cursor: Option<Value>,
}

/// Uniform record access used by `core.sync` and the generic record actions.
#[async_trait]
pub trait RecordObjects: Send + Sync {
    async fn list_objects(&self, ctx: &ActionCtx) -> Result<Vec<ObjectInfo>, ConnectorError>;
    async fn describe(
        &self,
        ctx: &ActionCtx,
        object: &str,
    ) -> Result<Vec<FieldSpec>, ConnectorError>;
    async fn get(
        &self,
        ctx: &ActionCtx,
        object: &str,
        id: &str,
    ) -> Result<Option<Record>, ConnectorError>;
    async fn find_by(
        &self,
        ctx: &ActionCtx,
        object: &str,
        field: &str,
        value: &Value,
    ) -> Result<Vec<Record>, ConnectorError>;
    async fn create(
        &self,
        ctx: &ActionCtx,
        object: &str,
        values: &Map<String, Value>,
    ) -> Result<RecordRef, ConnectorError>;
    async fn update(
        &self,
        ctx: &ActionCtx,
        object: &str,
        id: &str,
        values: &Map<String, Value>,
    ) -> Result<RecordRef, ConnectorError>;
    /// Records modified after `cursor` (oldest first). `cursor = None` starts from the beginning.
    async fn changed_since(
        &self,
        ctx: &ActionCtx,
        object: &str,
        cursor: Option<&Value>,
        limit: u32,
    ) -> Result<Page, ConnectorError>;
}
