//! weft-http: the production `HttpSender` (reqwest + SSRF guard). See `docs/plan/08-security.md` §5.

mod policy;
mod sender;

pub use policy::SsrfPolicy;
pub use sender::ReqwestSender;
