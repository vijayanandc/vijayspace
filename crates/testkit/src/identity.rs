//! An in-process identity provider (doc 09 §7.1): tokens issued by the test map to identities;
//! anything else is signed out. It can also be "down" to test provider outages.

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use async_trait::async_trait;
use weft_app::{
    AppError, ErrorKind,
    ports::{IdentityVerifier, Res, VerifiedIdentity},
};

#[derive(Debug, Clone, Default)]
pub struct FakeIdentityProvider {
    tokens: Arc<Mutex<HashMap<String, VerifiedIdentity>>>,
    down: Arc<AtomicBool>,
}

impl FakeIdentityProvider {
    /// Issues `token` for a person with a verified email (`subject` = `sub-<email>`).
    pub fn issue(&self, token: &str, email: &str, name: &str) {
        self.issue_identity(
            token,
            VerifiedIdentity {
                subject: format!("sub-{email}"),
                email: Some(email.to_owned()),
                email_verified: true,
                name: Some(name.to_owned()),
                has_profile: true,
            },
        );
    }

    pub fn issue_identity(&self, token: &str, identity: VerifiedIdentity) {
        self.tokens
            .lock()
            .unwrap()
            .insert(token.to_owned(), identity);
    }

    pub fn revoke(&self, token: &str) {
        self.tokens.lock().unwrap().remove(token);
    }

    pub fn set_down(&self, down: bool) {
        self.down.store(down, Ordering::SeqCst);
    }
}

#[async_trait]
impl IdentityVerifier for FakeIdentityProvider {
    async fn verify(&self, token: &str) -> Res<Option<VerifiedIdentity>> {
        if self.down.load(Ordering::SeqCst) {
            return Err(AppError::new(
                ErrorKind::BadGateway,
                weft_domain::error_codes::AUTH_UNAVAILABLE,
                "provider down",
            ));
        }
        Ok(self.tokens.lock().unwrap().get(token).cloned())
    }
}
