//! `LinkRepo` for MySQL: record links and per-record leases.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;
use weft_app::{
    AppError,
    ports::{LinkRepo, RecordLink, RecordSide, Res},
};
use weft_domain::{
    error_codes,
    ids::{ConnectionId, LinkId, OrgId},
};

use crate::{
    MySqlStore,
    codec::{db_err, is_duplicate},
};

const COLUMNS: &str = "id, org_id, link_set, a_connection_id, a_object, a_record_id, b_connection_id, b_object, b_record_id, b_hash, b_written_at";

#[derive(Debug, FromRow)]
struct LinkRow {
    id: Uuid,
    org_id: Uuid,
    link_set: String,
    a_connection_id: Uuid,
    a_object: String,
    a_record_id: String,
    b_connection_id: Uuid,
    b_object: String,
    b_record_id: String,
    b_hash: Option<Vec<u8>>,
    b_written_at: Option<DateTime<Utc>>,
}

impl From<LinkRow> for RecordLink {
    fn from(r: LinkRow) -> Self {
        Self {
            id: LinkId::from_uuid(r.id),
            org_id: OrgId::from_uuid(r.org_id),
            link_set: r.link_set,
            a: RecordSide {
                connection_id: ConnectionId::from_uuid(r.a_connection_id),
                object: r.a_object,
                record_id: r.a_record_id,
            },
            b: RecordSide {
                connection_id: ConnectionId::from_uuid(r.b_connection_id),
                object: r.b_object,
                record_id: r.b_record_id,
            },
            b_hash: r.b_hash,
            b_written_at: r.b_written_at,
        }
    }
}

fn find_by_a() -> String {
    format!(
        "SELECT {COLUMNS} FROM record_links WHERE org_id = ? AND link_set = ? AND a_connection_id = ? AND a_object = ? AND a_record_id = ?"
    )
}

fn find_by_b() -> String {
    format!(
        "SELECT {COLUMNS} FROM record_links WHERE org_id = ? AND link_set = ? AND b_connection_id = ? AND b_object = ? AND b_record_id = ?"
    )
}

impl MySqlStore {
    /// `sql`: [`find_by_a`] or [`find_by_b`] (the side's columns are fixed in the statement, never interpolated).
    async fn find_link(
        &self,
        sql: &str,
        org: OrgId,
        link_set: &str,
        s: &RecordSide,
    ) -> Res<Option<RecordLink>> {
        let row: Option<LinkRow> = sqlx::query_as(sql)
            .bind(org.as_uuid())
            .bind(link_set)
            .bind(s.connection_id.as_uuid())
            .bind(&s.object)
            .bind(&s.record_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(row.map(RecordLink::from))
    }
}

#[async_trait]
impl LinkRepo for MySqlStore {
    async fn find_link_by_a(
        &self,
        org: OrgId,
        link_set: &str,
        a: &RecordSide,
    ) -> Res<Option<RecordLink>> {
        self.find_link(&find_by_a(), org, link_set, a).await
    }

    async fn find_link_by_b(
        &self,
        org: OrgId,
        link_set: &str,
        b: &RecordSide,
    ) -> Res<Option<RecordLink>> {
        self.find_link(&find_by_b(), org, link_set, b).await
    }

    async fn upsert_link(&self, l: &RecordLink, now: DateTime<Utc>) -> Res<()> {
        // A concurrent insert for the same source record can win between our SELECT and INSERT
        // (READ COMMITTED takes no gap lock): retry once, which then takes the update path.
        let mut attempt = 0;
        loop {
            match self.save_link_once(l, now).await {
                Ok(()) => return Ok(()),
                Err(e)
                    if is_duplicate(&e)
                        && duplicate_key_is(&e, "uq_record_links_a")
                        && attempt == 0 =>
                {
                    attempt += 1;
                }
                // Side B already belongs to another source record.
                Err(e) if is_duplicate(&e) => {
                    return Err(AppError::conflict(
                        error_codes::SYNC_AMBIGUOUS_MATCH,
                        "The target record is already linked to another source record",
                    ));
                }
                Err(e) => return Err(db_err(e)),
            }
        }
    }

    async fn delete_link(&self, id: LinkId) -> Res<()> {
        sqlx::query("DELETE FROM record_links WHERE id = ?")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(())
    }

    async fn acquire_lease(
        &self,
        org: OrgId,
        key: &[u8],
        holder: &str,
        until: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Res<bool> {
        // Take the lease when it is free, expired or already ours (assignments run left to right).
        sqlx::query(
            "INSERT INTO sync_leases (org_id, lease_key, holder, locked_until) VALUES (?, ?, ?, ?) \
             ON DUPLICATE KEY UPDATE holder = IF(locked_until < ? OR holder = ?, ?, holder), locked_until = IF(holder = ?, ?, locked_until)",
        )
        .bind(org.as_uuid())
        .bind(key)
        .bind(holder)
        .bind(until)
        .bind(now)
        .bind(holder)
        .bind(holder)
        .bind(holder)
        .bind(until)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        let current: Option<String> =
            sqlx::query_scalar("SELECT holder FROM sync_leases WHERE org_id = ? AND lease_key = ?")
                .bind(org.as_uuid())
                .bind(key)
                .fetch_optional(&self.pool)
                .await
                .map_err(db_err)?;
        Ok(current.as_deref() == Some(holder))
    }

    async fn release_lease(&self, org: OrgId, key: &[u8], holder: &str) -> Res<()> {
        sqlx::query("DELETE FROM sync_leases WHERE org_id = ? AND lease_key = ? AND holder = ?")
            .bind(org.as_uuid())
            .bind(key)
            .bind(holder)
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(())
    }
}

/// True when a duplicate-key error is on the named unique key.
fn duplicate_key_is(e: &sqlx::Error, key: &str) -> bool {
    e.as_database_error()
        .is_some_and(|d| d.message().contains(key))
}

impl MySqlStore {
    /// One attempt: update the link of this source record, or insert it. Not
    /// `INSERT … ON DUPLICATE KEY UPDATE`: with two unique keys MySQL would update the row that
    /// collides on side B (another source record's link) instead of failing.
    async fn save_link_once(&self, l: &RecordLink, now: DateTime<Utc>) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let existing: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM record_links WHERE org_id = ? AND link_set = ? AND a_connection_id = ? AND a_object = ? AND a_record_id = ? FOR UPDATE",
        )
        .bind(l.org_id.as_uuid())
        .bind(&l.link_set)
        .bind(l.a.connection_id.as_uuid())
        .bind(&l.a.object)
        .bind(&l.a.record_id)
        .fetch_optional(&mut *tx)
        .await?;
        match existing {
            Some(id) => {
                sqlx::query("UPDATE record_links SET b_connection_id = ?, b_object = ?, b_record_id = ?, b_hash = ?, b_written_at = ?, updated_at = ? WHERE id = ?")
                    .bind(l.b.connection_id.as_uuid())
                    .bind(&l.b.object)
                    .bind(&l.b.record_id)
                    .bind(&l.b_hash)
                    .bind(l.b_written_at)
                    .bind(now)
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
            }
            None => {
                sqlx::query("INSERT INTO record_links (id, org_id, link_set, a_connection_id, a_object, a_record_id, b_connection_id, b_object, b_record_id, b_hash, b_written_at, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
                    .bind(l.id.as_uuid())
                    .bind(l.org_id.as_uuid())
                    .bind(&l.link_set)
                    .bind(l.a.connection_id.as_uuid())
                    .bind(&l.a.object)
                    .bind(&l.a.record_id)
                    .bind(l.b.connection_id.as_uuid())
                    .bind(&l.b.object)
                    .bind(&l.b.record_id)
                    .bind(&l.b_hash)
                    .bind(l.b_written_at)
                    .bind(now)
                    .bind(now)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        tx.commit().await
    }
}
