//! weft-infra: non-database adapters (clock, IDs, secret encryption, email, script runner).

mod crypto;
mod mail;
mod scripts;
mod system;

pub use crypto::AesGcmSecretBox;
pub use mail::{LogMailer, SmtpMailer};
pub use scripts::{PooledScriptRunner, script_error};
pub use system::{SystemClock, UuidV7Ids};
