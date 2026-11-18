//! `cargo xtask check-arch`: the architecture rules of AGENTS.md and `docs/plan/02-architecture.md` §4, checked.
//! Crate dependencies, where the clock, IDs, randomness, `tokio::spawn` and SQL may appear, SQL hygiene, file
//! headers and sizes, and migration names. Each problem says how to fix it. Std only, so it builds in seconds.

// A command-line tool reports on stdout/stderr.
#![allow(clippy::print_stdout, clippy::print_stderr)]

mod crates;
mod source;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// One rule violation: where, which rule, and what to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub path: String,
    pub line: usize,
    pub rule: &'static str,
    pub message: String,
}

fn main() -> ExitCode {
    let task = std::env::args().nth(1).unwrap_or_default();
    if task != "check-arch" {
        eprintln!("usage: cargo xtask check-arch");
        return ExitCode::FAILURE;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    match run(&root) {
        Ok(problems) if problems.is_empty() => {
            println!("Architecture check: OK");
            ExitCode::SUCCESS
        }
        Ok(problems) => {
            eprintln!("Architecture check: {} problem(s)", problems.len());
            for p in &problems {
                eprintln!("{}:{}  [{}] {}", p.path, p.line, p.rule, p.message);
            }
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("Architecture check could not run: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(root: &Path) -> Result<Vec<Problem>, String> {
    let mut problems = crates::check(root)?;
    problems.extend(source::check(root)?);
    Ok(problems)
}

/// Every file under `dir` (recursively) whose name ends with `ext`, sorted.
pub fn files(dir: &Path, ext: &str) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir() {
            out.extend(files(&path, ext)?);
        } else if path.to_string_lossy().ends_with(ext) {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

/// `path` relative to `root`, with `/` separators.
pub fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}
