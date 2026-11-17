//! weft-infra: non-database adapters (clock, IDs, secret encryption, email).

mod crypto;
mod mail;
mod system;

pub use crypto::AesGcmSecretBox;
pub use mail::{LogMailer, SmtpMailer};
pub use system::{SystemClock, UuidV7Ids};
