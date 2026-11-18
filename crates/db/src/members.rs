//! Membership changes that must never leave an org without an owner. Every change locks all of the
//! org's member rows in one statement (primary-key order), so concurrent changes queue up instead
//! of deadlocking, and the owner count it sees is exact.

use chrono::{DateTime, Utc};
use sqlx::{MySql, Transaction};
use uuid::Uuid;
use weft_app::ports::{MemberChange, Res};
use weft_domain::{
    ids::{OrgId, UserId},
    permissions::Role,
};

use crate::{MySqlStore, codec::db_err, tenancy_rows::role as parse_role};

/// Locks the org's members and decides whether `user`, whose role must still be `from`, may change
/// (`stays_owner`: whether they remain an owner afterwards). `Ok(None)` = go ahead.
async fn check(
    tx: &mut Transaction<'_, MySql>,
    org: OrgId,
    user: UserId,
    from: Role,
    stays_owner: bool,
) -> Res<Option<MemberChange>> {
    let rows: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT user_id, role FROM org_members WHERE org_id = ? FOR UPDATE")
            .bind(org.as_uuid())
            .fetch_all(&mut **tx)
            .await
            .map_err(db_err)?;
    let mut owners = 0_usize;
    let mut current = None;
    for (id, text) in rows {
        let r = parse_role(&text)?;
        if r == Role::Owner {
            owners += 1;
        }
        if id == user.as_uuid() {
            current = Some(r);
        }
    }
    if current != Some(from) {
        return Ok(Some(MemberChange::NotFound));
    }
    if from == Role::Owner && !stays_owner && owners <= 1 {
        return Ok(Some(MemberChange::LastOwner));
    }
    Ok(None)
}

impl MySqlStore {
    pub(crate) async fn claim_owner(
        &self,
        org: OrgId,
        user: UserId,
        now: DateTime<Utc>,
    ) -> Res<bool> {
        let mut tx = self.pool.begin().await.map_err(db_err)?;
        let roles: Vec<String> =
            sqlx::query_scalar("SELECT role FROM org_members WHERE org_id = ? FOR UPDATE")
                .bind(org.as_uuid())
                .fetch_all(&mut *tx)
                .await
                .map_err(db_err)?;
        if roles.iter().any(|r| r == Role::Owner.as_str()) {
            return Ok(false);
        }
        sqlx::query(
            "INSERT INTO org_members (org_id, user_id, role, created_at, updated_at) VALUES (?, ?, 'owner', ?, ?) \
             ON DUPLICATE KEY UPDATE role = 'owner', updated_at = ?",
        )
        .bind(org.as_uuid())
        .bind(user.as_uuid())
        .bind(now)
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;
        tx.commit().await.map_err(db_err)?;
        Ok(true)
    }

    pub(crate) async fn change_role(
        &self,
        org: OrgId,
        user: UserId,
        from: Role,
        to: Role,
        now: DateTime<Utc>,
    ) -> Res<MemberChange> {
        let mut tx = self.pool.begin().await.map_err(db_err)?;
        if let Some(refused) = check(&mut tx, org, user, from, to == Role::Owner).await? {
            return Ok(refused);
        }
        sqlx::query(
            "UPDATE org_members SET role = ?, updated_at = ? WHERE org_id = ? AND user_id = ?",
        )
        .bind(to.as_str())
        .bind(now)
        .bind(org.as_uuid())
        .bind(user.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;
        tx.commit().await.map_err(db_err)?;
        Ok(MemberChange::Done)
    }

    pub(crate) async fn delete_member(
        &self,
        org: OrgId,
        user: UserId,
        from: Role,
    ) -> Res<MemberChange> {
        let mut tx = self.pool.begin().await.map_err(db_err)?;
        if let Some(refused) = check(&mut tx, org, user, from, false).await? {
            return Ok(refused);
        }
        sqlx::query("DELETE FROM org_members WHERE org_id = ? AND user_id = ?")
            .bind(org.as_uuid())
            .bind(user.as_uuid())
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        tx.commit().await.map_err(db_err)?;
        Ok(MemberChange::Done)
    }
}
