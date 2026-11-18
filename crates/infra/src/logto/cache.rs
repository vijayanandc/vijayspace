//! Verified identities cached by token hash (never past the token's expiry), so a page that makes
//! a dozen requests asks Logto once.

use std::{collections::HashMap, sync::Mutex};

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use weft_app::ports::VerifiedIdentity;

/// SHA-256 of a token: the cache never holds tokens themselves.
pub(super) fn token_key(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}

/// An identity and when it stops being trusted.
type Entry = (VerifiedIdentity, DateTime<Utc>);

#[derive(Debug)]
pub(super) struct TokenCache {
    capacity: usize,
    entries: Mutex<HashMap<[u8; 32], Entry>>,
}

impl TokenCache {
    pub(super) fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: Mutex::new(HashMap::new()),
        }
    }

    pub(super) fn get(&self, key: &[u8; 32], now: DateTime<Utc>) -> Option<VerifiedIdentity> {
        let mut entries = self.entries.lock().ok()?;
        match entries.get(key) {
            Some((identity, until)) if *until > now => Some(identity.clone()),
            Some(_) => {
                entries.remove(key);
                None
            }
            None => None,
        }
    }

    /// Keeps `identity` until `until`. When full, expired entries go first, then the ones that
    /// expire soonest.
    pub(super) fn put(
        &self,
        key: [u8; 32],
        identity: VerifiedIdentity,
        until: DateTime<Utc>,
        now: DateTime<Utc>,
    ) {
        if self.capacity == 0 || until <= now {
            return;
        }
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };
        if entries.len() >= self.capacity {
            entries.retain(|_, (_, t)| *t > now);
        }
        while entries.len() >= self.capacity {
            let Some(soonest) = entries.iter().min_by_key(|(_, (_, t))| *t).map(|(k, _)| *k) else {
                break;
            };
            entries.remove(&soonest);
        }
        entries.insert(key, (identity, until));
    }
}
