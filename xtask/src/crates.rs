//! Crate dependency rules (`docs/plan/02-architecture.md` §4): which workspace crates each crate may depend on, and
//! which IO crates it must never use. Only `[dependencies]` count; tests may use more.

use std::path::Path;

use crate::{Problem, relative};

/// `(crate directory, allowed workspace crates, forbidden external crates)`.
pub const RULES: &[(&str, &[&str], &[&str])] = &[
    ("expr", &[], IO),
    ("domain", &["expr"], IO),
    ("engine", &["domain", "expr"], IO),
    ("script", &["expr"], IO),
    (
        "connector-sdk",
        &["domain", "expr"],
        &["sqlx", "axum", "lettre"],
    ),
    (
        "connectors",
        &["connector-sdk", "domain", "expr", "script"],
        &["sqlx", "axum", "lettre", "reqwest"],
    ),
    ("http", &["connector-sdk"], &["sqlx", "axum", "lettre"]),
    (
        "app",
        &["domain", "expr", "engine", "connector-sdk"],
        &["sqlx", "axum", "reqwest", "lettre"],
    ),
    (
        "db",
        &["app", "domain", "engine", "connector-sdk"],
        &["axum", "reqwest", "lettre"],
    ),
    (
        "infra",
        &["app", "domain", "connector-sdk", "script"],
        &["sqlx", "axum"],
    ),
    (
        "api",
        &["app", "domain", "connector-sdk"],
        &["sqlx", "reqwest", "lettre"],
    ),
    (
        "worker",
        &["app", "domain"],
        &["sqlx", "axum", "reqwest", "lettre"],
    ),
    (
        "testkit",
        &["domain", "expr", "engine", "connector-sdk", "app"],
        &["sqlx"],
    ),
    ("server", &["*"], &[]),
];

/// Pure crates: no async runtime, database, HTTP or mail.
const IO: &[&str] = &["tokio", "sqlx", "reqwest", "axum", "hyper", "lettre"];

pub fn check(root: &Path) -> Result<Vec<Problem>, String> {
    let mut problems = Vec::new();
    let dir = root.join("crates");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .filter(|e| e.path().join("Cargo.toml").is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    for name in names {
        let manifest = dir.join(&name).join("Cargo.toml");
        let text = std::fs::read_to_string(&manifest).map_err(|e| e.to_string())?;
        problems.extend(check_crate(&name, &text, &relative(root, &manifest)));
    }
    Ok(problems)
}

/// The crate names under `[dependencies]` (not dev or build dependencies), with their line numbers.
pub fn dependencies(manifest: &str) -> Vec<(String, usize)> {
    let mut out = Vec::new();
    let mut inside = false;
    for (i, line) in manifest.lines().enumerate() {
        let line = line.trim();
        if line.starts_with('[') {
            inside = line == "[dependencies]";
            continue;
        }
        if !inside || line.is_empty() || line.starts_with('#') {
            continue;
        }
        let name: String = line
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        if !name.is_empty() {
            out.push((name, i + 1));
        }
    }
    out
}

pub fn check_crate(name: &str, manifest: &str, path: &str) -> Vec<Problem> {
    let problem = |line, message| Problem {
        path: path.to_owned(),
        line,
        rule: "crates",
        message,
    };
    let Some((_, allowed, forbidden)) = RULES.iter().find(|(n, _, _)| *n == name) else {
        return vec![problem(
            1,
            format!(
                "crate `{name}` has no dependency rule: add it to xtask/src/crates.rs and docs/plan/02-architecture.md §4"
            ),
        )];
    };
    let mut problems = Vec::new();
    for (dep, line) in dependencies(manifest) {
        if let Some(internal) = dep.strip_prefix("weft-") {
            if !allowed.contains(&"*") && !allowed.contains(&internal) {
                problems.push(problem(line, format!("`{name}` may not depend on `{dep}` (allowed: {}): move the code to a crate both may use, or add a port trait in weft-app", allowed.join(", "))));
            }
        } else if forbidden.contains(&dep.as_str()) {
            problems.push(problem(
                line,
                format!(
                    "`{name}` must not use `{dep}`: IO belongs in db/infra/api/server behind a port"
                ),
            ));
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = "[package]\nname = \"weft-app\"\n\n[dependencies]\nweft-domain.workspace = true\nweft-db = { path = \"../db\" }\nsqlx.workspace = true\nserde = \"1\"\n\n[dev-dependencies]\nweft-testkit.workspace = true\nsqlx.workspace = true\n";

    #[test]
    fn reads_only_normal_dependencies() {
        let deps: Vec<String> = dependencies(MANIFEST).into_iter().map(|(d, _)| d).collect();
        assert_eq!(deps, ["weft-domain", "weft-db", "sqlx", "serde"]);
    }

    #[test]
    fn flags_disallowed_workspace_and_io_crates() {
        let problems = check_crate("app", MANIFEST, "crates/app/Cargo.toml");
        let lines: Vec<usize> = problems.iter().map(|p| p.line).collect();
        assert_eq!(lines, [6, 7], "weft-db and sqlx: {problems:?}");
        assert!(
            check_crate("server", MANIFEST, "x").is_empty(),
            "the composition root may use everything"
        );
        assert_eq!(
            check_crate("newcrate", "", "x").len(),
            1,
            "every crate needs a rule"
        );
    }
}
