//! Ports: Sync record links and per-record leases.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use weft_domain::ids::{ConnectionId, LinkId, OrgId};

use super::Res;

/// One side of a record link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordSide {
    pub connection_id: ConnectionId,
    pub object: String,
    pub record_id: String,
}

/// A link between a source record (side A) and a target record (side B).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordLink {
    pub id: LinkId,
    pub org_id: OrgId,
    pub link_set: String,
    pub a: RecordSide,
    pub b: RecordSide,
    pub b_hash: Option<Vec<u8>>,
    pub b_written_at: Option<DateTime<Utc>>,
}

#[async_trait]
pub trait LinkRepo: Send + Sync {
    async fn find_link_by_a(
        &self,
        org: OrgId,
        link_set: &str,
        a: &RecordSide,
    ) -> Res<Option<RecordLink>>;
    async fn find_link_by_b(
        &self,
        org: OrgId,
        link_set: &str,
        b: &RecordSide,
    ) -> Res<Option<RecordLink>>;
    /// Inserts or updates by side A. Fails with a conflict if side B is already linked to another A.
    async fn upsert_link(&self, link: &RecordLink, now: DateTime<Utc>) -> Res<()>;
    async fn delete_link(&self, id: LinkId) -> Res<()>;
    async fn acquire_lease(
        &self,
        org: OrgId,
        key: &[u8],
        holder: &str,
        until: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Res<bool>;
    async fn release_lease(&self, org: OrgId, key: &[u8], holder: &str) -> Res<()>;
}
