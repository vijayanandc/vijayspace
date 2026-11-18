//! Use cases: inviting people by email, and accepting or declining an invitation. An invitation is
//! accepted by signing in with the invited address, verified by Logto (doc 13 §6).

use chrono::Duration;
use serde::Serialize;
use weft_connector_sdk::EmailMessage;
use weft_domain::{
    error_codes,
    ids::InvitationId,
    model::{Invitation, Membership, User},
    permissions::{Permission, Role, can_assign},
};

use super::{Actor, normalize_email};
use crate::{AppError, Deps, ports::NewInvitation};

/// How long an invitation stays open.
const VALID_DAYS: i64 = 14;
/// Invitation emails an org may send per day (each invitation, renewal or resend sends one).
const DAILY_EMAILS: u32 = 100;
/// Invitation emails one person may send per day across all their orgs.
const DAILY_EMAILS_PER_USER: u32 = 300;
/// The shortest time between two emails for the same invitation.
const RESEND_COOLDOWN_MINUTES: i64 = 5;

fn rate_limited(message: &str) -> AppError {
    AppError::conflict(error_codes::INVITATION_RATE_LIMITED, message)
}

/// An invitation and whether its email went out.
#[derive(Debug, Clone, Serialize)]
pub struct Invited {
    pub invitation: Invitation,
    pub email_sent: bool,
}

fn not_found() -> AppError {
    AppError::not_found(error_codes::INVITATION_NOT_FOUND, "Invitation not found")
}

fn checked_email(email: &str) -> Result<String, AppError> {
    let email = normalize_email(email).unwrap_or_default();
    let valid = email.len() <= 320
        && !email.chars().any(char::is_whitespace)
        && email.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty() && domain.contains('.') && !domain.contains('@')
        });
    if valid {
        Ok(email)
    } else {
        Err(AppError::validation(
            error_codes::VALIDATION_FAILED,
            "Enter a valid email address",
        ))
    }
}

fn may_handle(actor: &Actor, role: Role) -> Result<(), AppError> {
    actor.require(Permission::MembersManage)?;
    if can_assign(actor.role, None, role) {
        Ok(())
    } else {
        Err(AppError::forbidden("Only an owner can invite owners"))
    }
}

pub async fn list(deps: &Deps, actor: &Actor) -> Result<Vec<Invitation>, AppError> {
    actor.require(Permission::MembersManage)?;
    deps.store
        .org_invitations(actor.org, deps.clock.now())
        .await
}

/// Invites an email to the org (or renews the open invitation for it) and emails a link.
pub async fn invite(
    deps: &Deps,
    actor: &Actor,
    email: &str,
    role: Role,
) -> Result<Invited, AppError> {
    may_handle(actor, role)?;
    let email = checked_email(email)?;
    if deps.store.is_member_email(actor.org, &email).await? {
        return Err(AppError::conflict(
            error_codes::MEMBER_EXISTS,
            format!("{email} is already a member"),
        ));
    }
    // Re-inviting renews the open invitation: the actor must be allowed to handle its role too
    // (an admin cannot turn an owner's invitation into something else).
    let open = deps
        .store
        .org_invitations(actor.org, deps.clock.now())
        .await?
        .into_iter()
        .find(|i| i.email == email);
    if let Some(open) = open {
        may_handle(actor, open.role)?;
        cooled_down(deps, &open)?;
    }
    issue(deps, actor, &email, role).await
}

/// Renews an open invitation's expiry and sends its email again.
pub async fn resend(deps: &Deps, actor: &Actor, id: InvitationId) -> Result<Invited, AppError> {
    let invitation = deps
        .store
        .org_invitation(actor.org, id)
        .await?
        .ok_or_else(not_found)?;
    may_handle(actor, invitation.role)?;
    cooled_down(deps, &invitation)?;
    issue(deps, actor, &invitation.email, invitation.role).await
}

/// One email per invitation every few minutes (inviting the same address again counts too).
fn cooled_down(deps: &Deps, invitation: &Invitation) -> Result<(), AppError> {
    if deps.clock.now() - invitation.created_at < Duration::minutes(RESEND_COOLDOWN_MINUTES) {
        return Err(rate_limited(
            "This invitation was just sent. Wait a few minutes before sending it again.",
        ));
    }
    Ok(())
}

pub async fn revoke(deps: &Deps, actor: &Actor, id: InvitationId) -> Result<(), AppError> {
    let invitation = deps
        .store
        .org_invitation(actor.org, id)
        .await?
        .ok_or_else(not_found)?;
    may_handle(actor, invitation.role)?;
    // Expired, not deleted: it keeps counting toward today's invitation emails.
    deps.store
        .expire_invitation(actor.org, id, deps.clock.now())
        .await?;
    Ok(())
}

async fn issue(deps: &Deps, actor: &Actor, email: &str, role: Role) -> Result<Invited, AppError> {
    let now = deps.clock.now();
    let mut quotas = vec![(format!("org:{}", actor.org), DAILY_EMAILS)];
    if let Some(user) = actor.user {
        quotas.push((format!("user:{user}"), DAILY_EMAILS_PER_USER));
    }
    if !deps
        .store
        .take_send_quota(&quotas, now.date_naive(), now)
        .await?
    {
        return Err(rate_limited(
            "The daily limit of invitation emails is reached. Try again tomorrow.",
        ));
    }
    deps.store
        .upsert_invitation(
            &NewInvitation {
                id: deps.new_invitation_id(),
                org_id: actor.org,
                email: email.to_owned(),
                role,
                invited_by: actor.user,
                expires_at: now + Duration::days(VALID_DAYS),
            },
            now,
        )
        .await?;
    let invitation = deps
        .store
        .org_invitations(actor.org, now)
        .await?
        .into_iter()
        .find(|i| i.email == email)
        .ok_or_else(not_found)?;
    let email_sent = send(deps, &invitation).await;
    Ok(Invited {
        invitation,
        email_sent,
    })
}

/// Sends the invitation email; a delivery failure is logged and reported, not fatal.
async fn send(deps: &Deps, invitation: &Invitation) -> bool {
    let Some(mailer) = &deps.mailer else {
        return false;
    };
    let message = invitation_email(&deps.config.app_url, invitation);
    match mailer.send(&message).await {
        Ok(()) => true,
        Err(e) => {
            tracing::warn!(error = %e, invitation = %invitation.id, "invitation email failed");
            false
        }
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// The invitation email (HTML). Names come from users, so they are escaped.
#[must_use]
pub fn invitation_email(app_url: &str, invitation: &Invitation) -> EmailMessage {
    let org = escape(&invitation.org_name);
    let by = invitation
        .invited_by
        .as_deref()
        .map_or_else(|| "Someone".to_owned(), escape);
    let link = escape(app_url.trim_end_matches('/'));
    let body = format!(
        "<div style=\"font-family:Segoe UI,Arial,sans-serif;font-size:14px;color:#242424\">\
         <p>{by} invited you to join <strong>{org}</strong> on Weft as {role}.</p>\
         <p><a href=\"{link}/\" style=\"display:inline-block;padding:8px 16px;background:#0f6cbd;\
         color:#fff;border-radius:4px;text-decoration:none\">Open Weft</a></p>\
         <p>Sign in with this email address ({email}) and accept the invitation. \
         It expires on {expires}.</p></div>",
        role = invitation.role,
        email = escape(&invitation.email),
        expires = invitation.expires_at.format("%d %b %Y"),
    );
    EmailMessage {
        to: vec![invitation.email.clone()],
        cc: Vec::new(),
        subject: format!("Join {} on Weft", invitation.org_name),
        body,
        html: true,
        reply_to: None,
    }
}

fn verified_email(user: Option<&User>) -> Result<(&User, &str), AppError> {
    let user = user.ok_or_else(|| {
        AppError::validation(
            error_codes::VALIDATION_FAILED,
            "Sign-in is turned off: there are no invitations",
        )
    })?;
    let email = user
        .email
        .as_deref()
        .filter(|_| user.email_verified)
        .ok_or_else(|| {
            AppError::forbidden("Verify your email address with your sign-in provider first")
        })?;
    Ok((user, email))
}

/// Accepts an invitation addressed to the signed-in user's verified email.
pub async fn accept(
    deps: &Deps,
    user: Option<&User>,
    id: InvitationId,
) -> Result<Membership, AppError> {
    let (user, email) = verified_email(user)?;
    deps.store
        .accept_invitation(id, email, user.id, deps.clock.now())
        .await?
        .ok_or_else(not_found)
}

/// Declines an invitation addressed to the signed-in user.
pub async fn decline(deps: &Deps, user: Option<&User>, id: InvitationId) -> Result<(), AppError> {
    let (_, email) = verified_email(user)?;
    let invitation = deps
        .store
        .invitations_for_email(email, deps.clock.now())
        .await?
        .into_iter()
        .find(|i| i.id == id)
        .ok_or_else(not_found)?;
    deps.store
        .delete_invitation(invitation.org_id, invitation.id)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone as _;
    use weft_domain::ids::OrgId;

    use super::*;

    #[test]
    fn emails_are_checked_and_lower_cased() {
        assert_eq!(
            checked_email(" Ann@Example.COM ").unwrap(),
            "ann@example.com"
        );
        for bad in [
            "",
            "ann",
            "@example.com",
            "ann@example",
            "a b@example.com",
            "a@b@c.com",
        ] {
            assert!(checked_email(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_invitation_email_escapes_names() {
        let at = chrono::Utc.with_ymd_and_hms(2026, 12, 2, 0, 0, 0).unwrap();
        let invitation = Invitation {
            id: InvitationId::from_uuid(uuid::Uuid::from_u128(1)),
            org_id: OrgId::from_uuid(uuid::Uuid::from_u128(2)),
            org_name: "Acme <Ops>".into(),
            email: "ann@example.com".into(),
            role: Role::Admin,
            invited_by: Some("Bo \"B\"".into()),
            created_at: at,
            expires_at: at,
        };
        let m = invitation_email("https://flow.example.com/", &invitation);
        assert_eq!(m.to, vec!["ann@example.com"]);
        assert_eq!(m.subject, "Join Acme <Ops> on Weft");
        assert!(m.html);
        assert!(m.body.contains("Acme &lt;Ops&gt;") && !m.body.contains("<Ops>"));
        assert!(m.body.contains("Bo &quot;B&quot;"));
        assert!(m.body.contains("href=\"https://flow.example.com/\""));
        assert!(m.body.contains("as admin") && m.body.contains("02 Dec 2026"));
    }
}
