//! Row types for users, memberships and invitations.

use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;
use weft_app::AppError;
use weft_domain::{
    ids::{InvitationId, OrgId, UserId},
    model::{Invitation, Member, Membership, Org, User},
    permissions::Role,
};

pub(crate) const USER_COLUMNS: &str =
    "u.id, u.logto_subject, u.email, u.email_verified, u.name, u.created_at";

pub(crate) const INVITATION_SELECT: &str = "SELECT i.id, i.org_id, o.name AS org_name, i.email, i.role, \
     COALESCE(u.name, u.email) AS invited_by, i.created_at, i.expires_at \
     FROM org_invitations i JOIN orgs o ON o.id = i.org_id LEFT JOIN users u ON u.id = i.invited_by";

pub(crate) fn role(text: &str) -> Result<Role, AppError> {
    text.parse()
        .map_err(|e: String| AppError::internal(format!("stored role: {e}")))
}

#[derive(Debug, FromRow)]
pub(crate) struct UserRow {
    pub id: Uuid,
    pub logto_subject: String,
    pub email: Option<String>,
    pub email_verified: bool,
    pub name: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl From<UserRow> for User {
    fn from(r: UserRow) -> Self {
        Self {
            id: UserId::from_uuid(r.id),
            logto_subject: r.logto_subject,
            email: r.email,
            email_verified: r.email_verified,
            name: r.name,
            created_at: r.created_at,
        }
    }
}

#[derive(Debug, FromRow)]
pub(crate) struct MemberRow {
    #[sqlx(flatten)]
    pub user: UserRow,
    pub org_id: Uuid,
    pub role: String,
    pub joined_at: DateTime<Utc>,
}

impl TryFrom<MemberRow> for Member {
    type Error = AppError;
    fn try_from(r: MemberRow) -> Result<Self, AppError> {
        Ok(Self {
            org_id: OrgId::from_uuid(r.org_id),
            role: role(&r.role)?,
            user: r.user.into(),
            joined_at: r.joined_at,
        })
    }
}

#[derive(Debug, FromRow)]
pub(crate) struct MembershipRow {
    pub id: Uuid,
    pub name: String,
    pub timezone: String,
    pub retention_days: Option<u32>,
    pub created_at: DateTime<Utc>,
    pub role: String,
}

impl TryFrom<MembershipRow> for Membership {
    type Error = AppError;
    fn try_from(r: MembershipRow) -> Result<Self, AppError> {
        Ok(Self {
            role: role(&r.role)?,
            org: Org {
                id: OrgId::from_uuid(r.id),
                name: r.name,
                timezone: r.timezone,
                retention_days: r.retention_days,
                created_at: r.created_at,
            },
        })
    }
}

#[derive(Debug, FromRow)]
pub(crate) struct InvitationRow {
    pub id: Uuid,
    pub org_id: Uuid,
    pub org_name: String,
    pub email: String,
    pub role: String,
    pub invited_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

impl TryFrom<InvitationRow> for Invitation {
    type Error = AppError;
    fn try_from(r: InvitationRow) -> Result<Self, AppError> {
        Ok(Self {
            id: InvitationId::from_uuid(r.id),
            org_id: OrgId::from_uuid(r.org_id),
            org_name: r.org_name,
            email: r.email,
            role: role(&r.role)?,
            invited_by: r.invited_by,
            created_at: r.created_at,
            expires_at: r.expires_at,
        })
    }
}
