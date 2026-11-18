//! Ports: the identity provider (Logto) and users, memberships and invitations (doc 13 §6).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use weft_domain::{
    ids::{InvitationId, OrgId, UserId},
    model::{Invitation, Member, Membership, Org, User},
    permissions::Role,
};

use super::Res;

/// What the identity provider vouches for. `email` is lower-cased.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedIdentity {
    pub subject: String,
    pub email: Option<String>,
    pub email_verified: bool,
    pub name: Option<String>,
}

/// Checks an access token with the identity provider.
#[async_trait]
pub trait IdentityVerifier: Send + Sync {
    /// `Ok(None)` for a token that is invalid, expired, revoked or for another API; `Err` when the
    /// provider could not be asked (the request is not rejected as signed out).
    async fn verify(&self, token: &str) -> Res<Option<VerifiedIdentity>>;
}

/// The outcome of changing or removing a membership.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberChange {
    Done,
    /// Not a member, or their role changed in the meantime.
    NotFound,
    /// The org would be left without an owner.
    LastOwner,
}

/// A new or renewed invitation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewInvitation {
    pub id: InvitationId,
    pub org_id: OrgId,
    pub email: String,
    pub role: Role,
    pub invited_by: Option<UserId>,
    pub expires_at: DateTime<Utc>,
}

#[async_trait]
pub trait TenancyRepo: Send + Sync {
    async fn user_by_subject(&self, subject: &str) -> Res<Option<User>>;
    /// Creates the user on first sign-in (`id` is used then), or refreshes their email and name.
    async fn upsert_user(
        &self,
        id: UserId,
        identity: &VerifiedIdentity,
        now: DateTime<Utc>,
    ) -> Res<User>;
    /// The user's orgs and roles, oldest membership first.
    async fn memberships(&self, user: UserId) -> Res<Vec<Membership>>;
    async fn member_role(&self, org: OrgId, user: UserId) -> Res<Option<Role>>;
    async fn list_members(&self, org: OrgId) -> Res<Vec<Member>>;
    /// Whether someone with this (verified) email is already a member.
    async fn is_member_email(&self, org: OrgId, email: &str) -> Res<bool>;
    /// Creates an org with its first owner, in one transaction.
    async fn create_org_with_owner(&self, org: &Org, owner: UserId, now: DateTime<Utc>) -> Res<()>;
    /// Adds a membership unless the user already has one (their role is kept).
    async fn ensure_member(
        &self,
        org: OrgId,
        user: UserId,
        role: Role,
        now: DateTime<Utc>,
    ) -> Res<()>;
    /// Changes a role if it is still `from`, unless that leaves the org without an owner.
    async fn set_member_role(
        &self,
        org: OrgId,
        user: UserId,
        from: Role,
        to: Role,
        now: DateTime<Utc>,
    ) -> Res<MemberChange>;
    /// Removes a member whose role is still `from`, unless that leaves the org without an owner.
    async fn remove_member(&self, org: OrgId, user: UserId, from: Role) -> Res<MemberChange>;
    /// Creates the invitation, or renews the open one for the same email (new role and expiry).
    async fn upsert_invitation(&self, invitation: &NewInvitation, now: DateTime<Utc>) -> Res<()>;
    async fn org_invitation(&self, org: OrgId, id: InvitationId) -> Res<Option<Invitation>>;
    /// Open (unexpired) invitations of an org, newest first.
    async fn org_invitations(&self, org: OrgId, now: DateTime<Utc>) -> Res<Vec<Invitation>>;
    /// Open invitations for an email, across orgs.
    async fn invitations_for_email(&self, email: &str, now: DateTime<Utc>) -> Res<Vec<Invitation>>;
    async fn delete_invitation(&self, org: OrgId, id: InvitationId) -> Res<bool>;
    /// Accepts an open invitation for `email`: adds the membership (an existing one keeps its
    /// role) and deletes the invitation, in one transaction. `None` if there is no such invitation.
    async fn accept_invitation(
        &self,
        id: InvitationId,
        email: &str,
        user: UserId,
        now: DateTime<Utc>,
    ) -> Res<Option<Membership>>;
}
