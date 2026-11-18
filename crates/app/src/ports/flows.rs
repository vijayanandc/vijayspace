//! Ports: Org and flow persistence.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use weft_domain::{
    flow::FlowDefinition,
    ids::{ExecutionId, FlowId, FlowVersionId, OrgId},
    model::{ExecutionStatus, Flow, FlowVersion, Org, TriggerRegistration},
};

use super::Res;

/// A flow row for list screens.
#[derive(Debug, Clone)]
pub struct FlowListRow {
    pub flow: Flow,
    pub last_execution: Option<(ExecutionId, ExecutionStatus, DateTime<Utc>)>,
}

#[async_trait]
pub trait OrgRepo: Send + Sync {
    /// Returns the default org, creating it on first use.
    async fn ensure_default_org(&self, id: OrgId, now: DateTime<Utc>) -> Res<Org>;
    async fn get_org(&self, id: OrgId) -> Res<Option<Org>>;
    async fn list_orgs(&self) -> Res<Vec<Org>>;
    /// `None` = the server default.
    async fn set_retention_days(&self, id: OrgId, days: Option<u32>, now: DateTime<Utc>)
    -> Res<()>;
    async fn update_org_profile(
        &self,
        id: OrgId,
        name: &str,
        timezone: &str,
        now: DateTime<Utc>,
    ) -> Res<()>;
}

#[async_trait]
pub trait FlowRepo: Send + Sync {
    /// Inserts a flow together with its trigger registration.
    async fn insert_flow(&self, flow: &Flow, registration: &TriggerRegistration) -> Res<()>;
    async fn get_flow(&self, org: OrgId, id: FlowId) -> Res<Option<Flow>>;
    /// Workers and ingress: no tenant scope.
    async fn get_flow_system(&self, id: FlowId) -> Res<Option<Flow>>;
    async fn list_flows(&self, org: OrgId) -> Res<Vec<FlowListRow>>;
    async fn update_flow_meta(
        &self,
        org: OrgId,
        id: FlowId,
        name: &str,
        description: Option<&str>,
        now: DateTime<Utc>,
    ) -> Res<()>;
    /// Saves the draft iff `expected_revision` matches; returns the new revision.
    async fn save_draft(
        &self,
        org: OrgId,
        id: FlowId,
        def: &FlowDefinition,
        expected_revision: u32,
        now: DateTime<Utc>,
    ) -> Res<u32>;
    async fn soft_delete_flow(&self, org: OrgId, id: FlowId, now: DateTime<Utc>) -> Res<()>;
    /// Inserts a published version (number = previous + 1) and points the flow at it.
    async fn publish_version(&self, version: &FlowVersion, now: DateTime<Utc>) -> Res<u32>;
    async fn insert_test_snapshot(&self, version: &FlowVersion) -> Res<()>;
    async fn get_version(&self, id: FlowVersionId) -> Res<Option<FlowVersion>>;
    async fn list_versions(&self, org: OrgId, flow: FlowId) -> Res<Vec<FlowVersion>>;
    async fn set_enabled(
        &self,
        org: OrgId,
        id: FlowId,
        enabled: bool,
        now: DateTime<Utc>,
    ) -> Res<()>;
}
