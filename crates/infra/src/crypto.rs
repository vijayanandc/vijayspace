//! AES-256-GCM secret box with a key ring (rotation) and AAD binding.

use std::collections::HashMap;

use aes_gcm::{
    Aes256Gcm, Key, Nonce,
    aead::{Aead, AeadCore, KeyInit, OsRng, Payload},
};
use base64::Engine as _;
use sha2::{Digest, Sha256};
use weft_app::{AppError, ports::SecretBox};

const NONCE_LEN: usize = 12;

/// Encrypts with the active key; decrypts with the key named by the stored key ID.
pub struct AesGcmSecretBox {
    keys: HashMap<String, Aes256Gcm>,
    active: String,
}

impl std::fmt::Debug for AesGcmSecretBox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AesGcmSecretBox")
            .field("active", &self.active)
            .field("keys", &self.keys.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl AesGcmSecretBox {
    /// Parses `id:base64key,id2:base64key2` (32-byte keys) and the active key ID.
    pub fn from_config(keys: &str, active: &str) -> Result<Self, String> {
        let mut ring = HashMap::new();
        for entry in keys.split(',').map(str::trim).filter(|e| !e.is_empty()) {
            let (id, b64) = entry.split_once(':').ok_or_else(|| {
                format!(
                    "key entry `{id}` must be id:base64",
                    id = entry.chars().take(8).collect::<String>()
                )
            })?;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(b64.trim())
                .map_err(|e| format!("key `{id}`: {e}"))?;
            if bytes.len() != 32 {
                return Err(format!("key `{id}` must be 32 bytes"));
            }
            ring.insert(
                id.to_owned(),
                Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&bytes)),
            );
        }
        if !ring.contains_key(active) {
            return Err(format!("active key `{active}` is not in the key ring"));
        }
        Ok(Self {
            keys: ring,
            active: active.to_owned(),
        })
    }

    /// A deterministic development key. Never use in production.
    #[must_use]
    pub fn development() -> Self {
        let key = Sha256::digest(b"weft-development-only-key");
        let mut keys = HashMap::new();
        keys.insert(
            "dev".to_owned(),
            Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key)),
        );
        Self {
            keys,
            active: "dev".to_owned(),
        }
    }
}

impl SecretBox for AesGcmSecretBox {
    fn seal(&self, plaintext: &[u8], aad: &[u8]) -> Result<(Vec<u8>, String), AppError> {
        let cipher = self
            .keys
            .get(&self.active)
            .ok_or_else(|| AppError::internal("active key missing"))?;
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ct = cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: plaintext,
                    aad,
                },
            )
            .map_err(|_| AppError::internal("encryption failed"))?;
        let mut out = Vec::with_capacity(NONCE_LEN + ct.len());
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&ct);
        Ok((out, self.active.clone()))
    }

    fn open(&self, ciphertext: &[u8], key_id: &str, aad: &[u8]) -> Result<Vec<u8>, AppError> {
        let id = if key_id.is_empty() {
            self.active.as_str()
        } else {
            key_id
        };
        let cipher = self
            .keys
            .get(id)
            .ok_or_else(|| AppError::internal(format!("unknown key `{id}`")))?;
        if ciphertext.len() <= NONCE_LEN {
            return Err(AppError::internal("ciphertext too short"));
        }
        let (nonce, ct) = ciphertext.split_at(NONCE_LEN);
        cipher
            .decrypt(Nonce::from_slice(nonce), Payload { msg: ct, aad })
            .map_err(|_| AppError::internal("decryption failed (wrong key or AAD)"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_aad_binding() {
        let b = AesGcmSecretBox::development();
        let (ct, kid) = b.seal(b"{\"token\":\"x\"}", b"org_1|cxn_1").unwrap();
        assert_eq!(
            b.open(&ct, &kid, b"org_1|cxn_1").unwrap(),
            b"{\"token\":\"x\"}"
        );
        assert!(
            b.open(&ct, &kid, b"org_2|cxn_1").is_err(),
            "a ciphertext moved to another row must not decrypt"
        );
        let mut tampered = ct.clone();
        if let Some(last) = tampered.last_mut() {
            *last ^= 1;
        }
        assert!(b.open(&tampered, &kid, b"org_1|cxn_1").is_err());
    }

    #[test]
    fn key_ring_parsing_and_rotation() {
        let k1 = base64::engine::general_purpose::STANDARD.encode([1u8; 32]);
        let k2 = base64::engine::general_purpose::STANDARD.encode([2u8; 32]);
        let old = AesGcmSecretBox::from_config(&format!("k1:{k1}"), "k1").unwrap();
        let (ct, kid) = old.seal(b"s", b"a").unwrap();
        let rotated = AesGcmSecretBox::from_config(&format!("k1:{k1},k2:{k2}"), "k2").unwrap();
        assert_eq!(rotated.open(&ct, &kid, b"a").unwrap(), b"s");
        assert_eq!(rotated.seal(b"s", b"a").unwrap().1, "k2");
        assert!(AesGcmSecretBox::from_config("k1:short", "k1").is_err());
        assert!(AesGcmSecretBox::from_config(&format!("k1:{k1}"), "k9").is_err());
    }
}
