//! The `script-child` process: reads one script request from stdin, runs it with the sandbox
//! limits under an address-space and CPU cap, and writes the response to stdout (doc 08 §7).

use std::io::{Read as _, Write as _};

use weft_script::{Limits, child};

/// Default address-space cap of a script process, in MB.
pub const DEFAULT_MEMORY_MB: u64 = 512;
/// CPU seconds a script process may use (a backstop to the engine's operation budget).
const CPU_SECS: u64 = 10;
/// The largest request accepted on stdin.
const MAX_REQUEST_BYTES: u64 = 16 << 20;

fn set_limits() {
    let memory_mb = std::env::var("WEFT_SCRIPT_MEMORY_MB")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_MEMORY_MB);
    let bytes = memory_mb.saturating_mul(1 << 20);
    // Best effort on platforms without these limits; the engine's own limits still apply.
    let _ = rlimit::setrlimit(rlimit::Resource::AS, bytes, bytes);
    let _ = rlimit::setrlimit(rlimit::Resource::CPU, CPU_SECS, CPU_SECS);
}

/// Runs the child protocol and exits.
pub fn run() -> anyhow::Result<()> {
    set_limits();
    let mut request = String::new();
    std::io::stdin()
        .take(MAX_REQUEST_BYTES)
        .read_to_string(&mut request)?;
    let response = child::respond(&request, &Limits::default());
    let mut out = std::io::stdout().lock();
    serde_json::to_writer(&mut out, &response)?;
    out.flush()?;
    Ok(())
}
