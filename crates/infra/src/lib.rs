//! weft-infra: non-database adapters (clock, IDs, secret encryption, email, Logto sign-in,
//! script runner).

mod crypto;
mod logto;
mod mail;
mod scripts;
mod system;
mod zeptomail;

pub use crypto::AesGcmSecretBox;
pub use logto::{LogtoSettings, LogtoVerifier};
pub use mail::{LogMailer, SmtpMailer};
pub use scripts::{CHILD_SUBCOMMAND, PooledScriptRunner, ProcessScriptRunner, script_error};
pub use system::{SystemClock, UuidV7Ids};
pub use zeptomail::{DEFAULT_API_URL as ZEPTOMAIL_API_URL, ZeptoMailer};
