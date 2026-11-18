//! `TenancyRepo` for MySQL: users (created on first sign-in), memberships and invitations.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use weft_app::{
    AppError,
    ports::{MemberChange, NewInvitation, Res, TenancyRepo, VerifiedIdentity},
};
use weft_domain::{
    ids::{InvitationId, OrgId, UserId},
    model::{Invitation, Member, Membership, Org, User},
    permissions::Role,
};

use crate::{
    MySqlStore,
    codec::db_err,
    tenancy_rows::{
        INVITATION_SELECT, InvitationRow, MemberRow, MembershipRow, USER_COLUMNS, UserRow, role,
    },
};

const MEMBERSHIPS: &str = "SELECT o.id, o.name, o.timezone, o.retention_days, o.created_at, m.role \
     FROM org_members m JOIN orgs o ON o.id = m.org_id";

fn invitations(rows: Vec<InvitationRow>) -> Res<Vec<Invitation>> {
    rows.into_iter().map(Invitation::try_from).collect()
}

#[async_trait]
impl TenancyRepo for MySqlStore {
    async fn user_by_subject(&self, subject: &str) -> Res<Option<User>> {
        let row: Option<UserRow> = sqlx::query_as(&format!(
            "SELECT {USER_COLUMNS} FROM users u WHERE u.logto_subject = ?"
        ))
        .bind(subject)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(row.map(User::from))
    }

    async fn upsert_user(&self, id: UserId, v: &VerifiedIdentity, now: DateTime<Utc>) -> Res<User> {
        sqlx::query(
            "INSERT INTO users (id, logto_subject, email, email_verified, name, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?) AS new \
             ON DUPLICATE KEY UPDATE email = new.email, email_verified = new.email_verified, \
             name = new.name, updated_at = new.updated_at",
        )
        .bind(id.as_uuid())
        .bind(&v.subject)
        .bind(&v.email)
        .bind(v.email_verified)
        .bind(&v.name)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        self.user_by_subject(&v.subject)
            .await?
            .ok_or_else(|| AppError::internal("user missing after upsert"))
    }

    async fn memberships(&self, user: UserId) -> Res<Vec<Membership>> {
        let rows: Vec<MembershipRow> = sqlx::query_as(&format!(
            "{MEMBERSHIPS} WHERE m.user_id = ? ORDER BY m.created_at, o.id LIMIT 500"
        ))
        .bind(user.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;
        rows.into_iter().map(Membership::try_from).collect()
    }

    async fn member_role(&self, org: OrgId, user: UserId) -> Res<Option<Role>> {
        let row: Option<String> =
            sqlx::query_scalar("SELECT role FROM org_members WHERE org_id = ? AND user_id = ?")
                .bind(org.as_uuid())
                .bind(user.as_uuid())
                .fetch_optional(&self.pool)
                .await
                .map_err(db_err)?;
        row.as_deref().map(role).transpose()
    }

    async fn list_members(&self, org: OrgId) -> Res<Vec<Member>> {
        let rows: Vec<MemberRow> = sqlx::query_as(&format!(
            "SELECT {USER_COLUMNS}, m.org_id, m.role, m.created_at AS joined_at \
             FROM org_members m JOIN users u ON u.id = m.user_id \
             WHERE m.org_id = ? ORDER BY m.created_at, u.id LIMIT 1000"
        ))
        .bind(org.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;
        rows.into_iter().map(Member::try_from).collect()
    }

    async fn is_member_email(&self, org: OrgId, email: &str) -> Res<bool> {
        let found: Option<i32> = sqlx::query_scalar(
            "SELECT 1 FROM org_members m JOIN users u ON u.id = m.user_id \
             WHERE m.org_id = ? AND u.email = ? LIMIT 1",
        )
        .bind(org.as_uuid())
        .bind(email)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(found.is_some())
    }

    async fn create_org_with_owner(&self, org: &Org, owner: UserId, now: DateTime<Utc>) -> Res<()> {
        let mut tx = self.pool.begin().await.map_err(db_err)?;
        sqlx::query("INSERT INTO orgs (id, name, timezone, is_default, created_at, updated_at) VALUES (?, ?, ?, FALSE, ?, ?)")
            .bind(org.id.as_uuid())
            .bind(&org.name)
            .bind(&org.timezone)
            .bind(now)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        sqlx::query("INSERT INTO org_members (org_id, user_id, role, created_at, updated_at) VALUES (?, ?, 'owner', ?, ?)")
            .bind(org.id.as_uuid())
            .bind(owner.as_uuid())
            .bind(now)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        tx.commit().await.map_err(db_err)
    }

    async fn claim_ownerless_org(&self, org: OrgId, user: UserId, now: DateTime<Utc>) -> Res<bool> {
        self.claim_owner(org, user, now).await
    }

    async fn set_member_role(
        &self,
        org: OrgId,
        user: UserId,
        from: Role,
        to: Role,
        now: DateTime<Utc>,
    ) -> Res<MemberChange> {
        self.change_role(org, user, from, to, now).await
    }

    async fn remove_member(&self, org: OrgId, user: UserId, from: Role) -> Res<MemberChange> {
        self.delete_member(org, user, from).await
    }

    async fn upsert_invitation(&self, i: &NewInvitation, now: DateTime<Utc>) -> Res<()> {
        sqlx::query(
            "INSERT INTO org_invitations (id, org_id, email, role, invited_by, created_at, expires_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?) AS new \
             ON DUPLICATE KEY UPDATE role = new.role, invited_by = new.invited_by, \
             created_at = new.created_at, expires_at = new.expires_at",
        )
        .bind(i.id.as_uuid())
        .bind(i.org_id.as_uuid())
        .bind(&i.email)
        .bind(i.role.as_str())
        .bind(i.invited_by.map(|u| u.as_uuid()))
        .bind(now)
        .bind(i.expires_at)
        .execute(&self.pool)
        .await
        .map_err(db_err)?;
        Ok(())
    }

    async fn org_invitation(&self, org: OrgId, id: InvitationId) -> Res<Option<Invitation>> {
        let row: Option<InvitationRow> = sqlx::query_as(&format!(
            "{INVITATION_SELECT} WHERE i.org_id = ? AND i.id = ?"
        ))
        .bind(org.as_uuid())
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?;
        row.map(Invitation::try_from).transpose()
    }

    async fn org_invitations(&self, org: OrgId, now: DateTime<Utc>) -> Res<Vec<Invitation>> {
        let rows: Vec<InvitationRow> = sqlx::query_as(&format!(
            "{INVITATION_SELECT} WHERE i.org_id = ? AND i.expires_at > ? ORDER BY i.created_at DESC LIMIT 500"
        ))
        .bind(org.as_uuid())
        .bind(now)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;
        invitations(rows)
    }

    async fn invitations_for_email(&self, email: &str, now: DateTime<Utc>) -> Res<Vec<Invitation>> {
        let rows: Vec<InvitationRow> = sqlx::query_as(&format!(
            "{INVITATION_SELECT} WHERE i.email = ? AND i.expires_at > ? ORDER BY i.created_at DESC LIMIT 50"
        ))
        .bind(email)
        .bind(now)
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;
        invitations(rows)
    }

    async fn delete_invitation(&self, org: OrgId, id: InvitationId) -> Res<bool> {
        let done = sqlx::query("DELETE FROM org_invitations WHERE org_id = ? AND id = ?")
            .bind(org.as_uuid())
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(done.rows_affected() > 0)
    }

    async fn expire_invitation(
        &self,
        org: OrgId,
        id: InvitationId,
        now: DateTime<Utc>,
    ) -> Res<bool> {
        let done = sqlx::query("UPDATE org_invitations SET expires_at = ? WHERE org_id = ? AND id = ? AND expires_at > ?")
            .bind(now)
            .bind(org.as_uuid())
            .bind(id.as_uuid())
            .bind(now)
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(done.rows_affected() > 0)
    }

    async fn take_send_quota(
        &self,
        quotas: &[(String, u32)],
        day: chrono::NaiveDate,
        now: DateTime<Utc>,
    ) -> Res<bool> {
        self.take_quota(quotas, day, now).await
    }

    async fn accept_invitation(
        &self,
        id: InvitationId,
        email: &str,
        user: UserId,
        now: DateTime<Utc>,
    ) -> Res<Option<Membership>> {
        let mut tx = self.pool.begin().await.map_err(db_err)?;
        let found: Option<(uuid::Uuid, String)> = sqlx::query_as(
            "SELECT org_id, role FROM org_invitations WHERE id = ? AND email = ? AND expires_at > ? FOR UPDATE",
        )
        .bind(id.as_uuid())
        .bind(email)
        .bind(now)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_err)?;
        let Some((org, invited_role)) = found else {
            return Ok(None);
        };
        sqlx::query(
            "INSERT INTO org_members (org_id, user_id, role, created_at, updated_at) VALUES (?, ?, ?, ?, ?) \
             ON DUPLICATE KEY UPDATE org_id = org_id",
        )
        .bind(org)
        .bind(user.as_uuid())
        .bind(&invited_role)
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;
        sqlx::query("DELETE FROM org_invitations WHERE id = ?")
            .bind(id.as_uuid())
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        let row: MembershipRow = sqlx::query_as(&format!(
            "{MEMBERSHIPS} WHERE m.org_id = ? AND m.user_id = ?"
        ))
        .bind(org)
        .bind(user.as_uuid())
        .fetch_one(&mut *tx)
        .await
        .map_err(db_err)?;
        tx.commit().await.map_err(db_err)?;
        Membership::try_from(row).map(Some)
    }
}
