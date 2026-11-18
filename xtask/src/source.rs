//! Source rules (AGENTS.md "Must" and "Never"): file headers and sizes, where the clock, IDs, randomness,
//! `tokio::spawn` and SQL may appear, SQL without `SELECT *` or database time, SQL strings that interpolate only
//! constant column lists, and migration file names.

use std::path::Path;

use crate::{Problem, files, relative};

const MAX_LINES: usize = 400;

/// Files allowed past the size limit, each with its reason.
const LARGE: &[(&str, &str)] = &[(
    "crates/engine/src/interpreter.rs",
    "🔒 engine: splitting it needs human review",
)];

/// `(text, paths where it may appear, how to fix it elsewhere)`, checked in production code (tests are free).
const PLACED: &[(&str, &[&str], &str)] = &[
    (
        "Utc::now()",
        &["crates/infra/src/system.rs"],
        "use `deps.clock.now()` so tests control time",
    ),
    (
        "Uuid::now_v7()",
        &["crates/infra/src/system.rs"],
        "use `deps.ids.*`",
    ),
    (
        "thread_rng()",
        // OAuth state and PKCE verifiers are secrets: OS-seeded randomness, never a test seam.
        &["crates/connector-sdk/src/oauth.rs"],
        "take randomness from a port (`deps.ids`)",
    ),
    (
        "tokio::spawn",
        &["crates/worker/", "crates/server/"],
        "background work belongs in weft-worker",
    ),
    (
        "sqlx::",
        &["crates/db/"],
        "SQL lives in crates/db behind a port",
    ),
];

/// Banned in SQL (crates/db): every column is named, and time is bound from the app clock.
const SQL_BANNED: &[(&str, &str)] = &[
    ("SELECT *", "name the columns"),
    ("NOW()", "bind `deps.clock.now()` with `?`"),
    ("CURRENT_TIMESTAMP", "bind `deps.clock.now()` with `?`"),
    ("UTC_TIMESTAMP", "bind `deps.clock.now()` with `?`"),
    ("SYSDATE()", "bind `deps.clock.now()` with `?`"),
];

pub fn check(root: &Path) -> Result<Vec<Problem>, String> {
    let mut problems = Vec::new();
    let mut paths = files(&root.join("crates"), ".rs")?;
    paths.extend(files(&root.join("xtask"), ".rs")?);
    for path in paths {
        let rel = relative(root, &path);
        if rel.contains("/target/") {
            continue;
        }
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{rel}: {e}"))?;
        problems.extend(check_file(&rel, &text));
    }
    problems.extend(check_migrations(root)?);
    Ok(problems)
}

/// Whether `rel` is production code (`crates/*/src`, not a `tests.rs` module).
fn is_production(rel: &str) -> bool {
    rel.starts_with("crates/") && rel.contains("/src/") && !rel.ends_with("/tests.rs")
}

/// The part of a file before its `#[cfg(test)]` module (tests sit at the end of a file by convention).
fn production_lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.lines()
        .enumerate()
        .take_while(|(_, l)| l.trim() != "#[cfg(test)]")
        .map(|(i, l)| (i + 1, l))
        .filter(|(_, l)| !l.trim_start().starts_with("//"))
}

pub fn check_file(rel: &str, text: &str) -> Vec<Problem> {
    let mut problems = Vec::new();
    let mut add = |line, rule, message: String| {
        problems.push(Problem {
            path: rel.to_owned(),
            line,
            rule,
            message,
        });
    };
    if !text.starts_with("//!") {
        add(
            1,
            "header",
            "start with a `//!` line saying what this file is".into(),
        );
    }
    if !is_production(rel) {
        return problems;
    }
    let count = text.lines().count();
    if count > MAX_LINES && !LARGE.iter().any(|(p, _)| *p == rel) {
        add(
            MAX_LINES + 1,
            "size",
            format!("{count} lines (max {MAX_LINES}): split it by responsibility"),
        );
    }
    for (n, line) in production_lines(text) {
        for (needle, allowed, fix) in PLACED {
            if line.contains(needle) && !allowed.iter().any(|a| rel.starts_with(a)) {
                add(
                    n,
                    "placement",
                    format!("`{needle}` is not allowed here: {fix}"),
                );
            }
        }
        if rel.starts_with("crates/db/") {
            let upper = line.to_uppercase();
            for (banned, fix) in SQL_BANNED {
                if upper.contains(banned) {
                    add(n, "sql", format!("`{banned}` in SQL: {fix}"));
                }
            }
        }
    }
    if rel.starts_with("crates/db/") {
        for (line, interpolated) in interpolated_sql(text) {
            add(
                line,
                "sql",
                format!(
                    "SQL built with `{{{interpolated}}}`: bind values with `?` (only constant column lists such as `{{COLUMNS}}` may be interpolated)"
                ),
            );
        }
    }
    problems
}

/// `format!` strings that contain SQL and interpolate something other than an `UPPER_CASE` constant.
fn interpolated_sql(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut rest = text;
    let mut offset = 0;
    while let Some(i) = rest.find("format!(") {
        let start = offset + i;
        let after = &text[start..];
        let literal = after
            .find('"')
            .and_then(|q| {
                after[q + 1..]
                    .find('"')
                    .map(|end| &after[q + 1..q + 1 + end])
            })
            .unwrap_or("");
        let is_sql = ["SELECT ", "INSERT ", "UPDATE ", "DELETE ", " WHERE "]
            .iter()
            .any(|k| literal.contains(k));
        if is_sql {
            for part in literal.split('{').skip(1) {
                let name = part.split('}').next().unwrap_or("");
                if name.is_empty() || !name.chars().all(|c| c.is_ascii_uppercase() || c == '_') {
                    out.push((text[..start].lines().count().max(1), name.to_owned()));
                }
            }
        }
        offset = start + "format!(".len();
        rest = &text[offset..];
    }
    out
}

fn check_migrations(root: &Path) -> Result<Vec<Problem>, String> {
    let mut problems = Vec::new();
    let mut previous = String::new();
    for path in files(&root.join("migrations"), ".sql")? {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let (stamp, rest) = name.split_at(name.find('_').unwrap_or(0));
        let named =
            stamp.len() == 14 && stamp.chars().all(|c| c.is_ascii_digit()) && rest.len() > 5;
        if !named || stamp <= previous.as_str() {
            problems.push(Problem {
                path: relative(root, &path),
                line: 1,
                rule: "migrations",
                message: "migrations are `YYYYMMDDHHMMSS_name.sql`, each later than the last (append a new one; never edit a merged one)".into(),
            });
        }
        stamp.clone_into(&mut previous);
    }
    Ok(problems)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(rel: &str, text: &str) -> Vec<&'static str> {
        check_file(rel, text).into_iter().map(|p| p.rule).collect()
    }

    #[test]
    fn production_code_keeps_the_clock_ids_spawning_and_sql_in_place() {
        assert_eq!(
            rules("crates/app/src/x.rs", "//! x\nlet t = Utc::now();\n"),
            ["placement"]
        );
        assert!(rules("crates/infra/src/system.rs", "//! x\nUtc::now()\n").is_empty());
        assert_eq!(
            rules(
                "crates/app/src/x.rs",
                "//! x\ntokio::spawn(f);\nsqlx::query(\"\");\n"
            ),
            ["placement", "placement"]
        );
        assert!(
            rules(
                "crates/app/src/x.rs",
                "//! x\n// tokio::spawn in a comment\n#[cfg(test)]\nmod t { Utc::now(); }\n"
            )
            .is_empty()
        );
        assert!(
            rules("crates/app/src/recipes/tests.rs", "//! x\nUtc::now();\n").is_empty(),
            "test modules are free"
        );
    }

    #[test]
    fn sql_names_its_columns_binds_time_and_interpolates_only_constants() {
        let db = "crates/db/src/x.rs";
        assert_eq!(
            rules(db, "//! x\nsqlx::query(\"SELECT * FROM flows\")\n"),
            ["sql"]
        );
        assert_eq!(
            rules(db, "//! x\n\"UPDATE jobs SET at = NOW()\"\n"),
            ["sql"]
        );
        assert!(
            rules(
                db,
                "//! x\nlet q = format!(\"SELECT {COLUMNS} FROM flows WHERE id = ?\");\n"
            )
            .is_empty()
        );
        assert_eq!(
            rules(
                db,
                "//! x\nlet q = format!(\n    \"SELECT id FROM flows WHERE name = '{name}'\"\n);\n"
            ),
            ["sql"]
        );
        assert!(
            rules(db, "//! x\nformat!(\"exe:{id}\")\n").is_empty(),
            "not SQL"
        );
    }

    #[test]
    fn files_need_a_header_and_stay_small() {
        assert_eq!(rules("crates/app/src/x.rs", "fn main() {}\n"), ["header"]);
        let long = format!("//! x\n{}", "let a = 1;\n".repeat(MAX_LINES));
        assert_eq!(rules("crates/app/src/x.rs", &long), ["size"]);
        assert!(
            rules("crates/server/tests/scenarios/x.rs", &long).is_empty(),
            "tests may be longer"
        );
    }
}
