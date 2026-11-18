//! Settings from a `config.properties` file with environment overrides, as the Appblocks servers do: each
//! `WEFT_FOO_BAR` variable can be written in the file as `foo_bar = …`, and a set (non-empty) environment
//! variable always wins over the file. The file path is `WEFT_CONFIG_PATH`, else `./config.properties` when it
//! exists. Secrets may live in the file: keep it readable by the server's user only.

use std::{collections::HashMap, path::Path};

/// Where settings come from: the environment first, then the file.
#[derive(Debug, Default, Clone)]
pub struct Source {
    file: HashMap<String, String>,
    path: Option<String>,
}

/// The file key for an environment name: `WEFT_LOGTO_ENDPOINT` → `logto_endpoint` (`DATABASE_URL` →
/// `database_url`).
#[must_use]
pub fn key_for(env_name: &str) -> String {
    env_name
        .strip_prefix("WEFT_")
        .unwrap_or(env_name)
        .to_ascii_lowercase()
}

/// Parses Java-style properties: `key = value` or `key: value` lines, `#` and `!` comments, blank lines.
/// Values are trimmed; a value may not continue on the next line.
pub fn parse(text: &str) -> Result<HashMap<String, String>, String> {
    let mut out = HashMap::new();
    for (n, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('!') {
            continue;
        }
        let Some(at) = line.find(['=', ':']) else {
            return Err(format!(
                "line {}: expected `key = value`, found `{line}`",
                n + 1
            ));
        };
        let key = line[..at].trim().to_ascii_lowercase();
        if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(format!(
                "line {}: `{}` is not a setting name",
                n + 1,
                &line[..at]
            ));
        }
        if out
            .insert(key.clone(), line[at + 1..].trim().to_owned())
            .is_some()
        {
            return Err(format!("line {}: `{key}` is set twice", n + 1));
        }
    }
    Ok(out)
}

impl Source {
    /// Reads the file (`WEFT_CONFIG_PATH`, else `./config.properties` if present). A named file that is
    /// missing or malformed stops the server.
    pub fn load() -> Result<Self, String> {
        let named = env("WEFT_CONFIG_PATH");
        let path = match named {
            Some(p) => p,
            None if Path::new("config.properties").is_file() => "config.properties".to_owned(),
            None => return Ok(Self::default()),
        };
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("reading config `{path}`: {e}"))?;
        let file = parse(&text).map_err(|e| format!("config `{path}`: {e}"))?;
        warn_if_readable_by_others(&path);
        Ok(Self {
            file,
            path: Some(path),
        })
    }

    /// A source with only these file entries (tests).
    #[must_use]
    pub fn from_file_entries(file: HashMap<String, String>) -> Self {
        Self { file, path: None }
    }

    /// The setting `env_name`: the environment variable when set, else the file's `key_for(env_name)`.
    #[must_use]
    pub fn get(&self, env_name: &str) -> Option<String> {
        env(env_name).or_else(|| {
            self.file
                .get(&key_for(env_name))
                .map(|v| v.trim().to_owned())
                .filter(|v| !v.is_empty())
        })
    }

    /// File keys that no setting reads (likely typos), given every name the server asked for.
    #[must_use]
    pub fn unknown_keys(&self, asked: &[String]) -> Vec<String> {
        let known: Vec<String> = asked.iter().map(|n| key_for(n)).collect();
        let mut unknown: Vec<String> = self
            .file
            .keys()
            .filter(|k| !known.contains(k))
            .cloned()
            .collect();
        unknown.sort();
        unknown
    }

    #[must_use]
    pub fn path(&self) -> Option<&str> {
        self.path.as_deref()
    }
}

fn env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
}

#[cfg(unix)]
fn warn_if_readable_by_others(path: &str) {
    use std::os::unix::fs::PermissionsExt as _;
    if let Ok(meta) = std::fs::metadata(path)
        && meta.permissions().mode() & 0o004 != 0
    {
        tracing::warn!(
            path,
            "the config file is readable by every user; it may hold secrets (chmod 600)"
        );
    }
}

#[cfg(not(unix))]
fn warn_if_readable_by_others(_path: &str) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn properties_parse_like_the_appblocks_files() {
        let file = parse(
            "# Weft\n! also a comment\n\ndatabase_url = mysql://u:p@db/weft\nLOGTO_ENDPOINT: https://t.logto.app\nempty =\n",
        )
        .unwrap();
        assert_eq!(file["database_url"], "mysql://u:p@db/weft");
        assert_eq!(file["logto_endpoint"], "https://t.logto.app");
        assert_eq!(file["empty"], "");
        assert!(parse("no separator here").is_err());
        assert!(parse("bad key! = x").is_err());
        assert!(
            parse("a = 1\na = 2").is_err(),
            "a key set twice is a mistake"
        );
    }

    #[test]
    fn the_environment_overrides_the_file() {
        let source = Source::from_file_entries(HashMap::from([
            ("retention_days".to_owned(), "14".to_owned()),
            ("empty".to_owned(), String::new()),
        ]));
        assert_eq!(key_for("WEFT_RETENTION_DAYS"), "retention_days");
        assert_eq!(key_for("DATABASE_URL"), "database_url");
        assert_eq!(source.get("WEFT_RETENTION_DAYS").as_deref(), Some("14"));
        assert_eq!(source.get("WEFT_EMPTY"), None, "an empty value is unset");
        // PATH is always set in the environment: it wins over a file entry.
        let path = Source::from_file_entries(HashMap::from([("path".to_owned(), "x".to_owned())]));
        assert_ne!(path.get("PATH").as_deref(), Some("x"));
        assert_eq!(
            source.unknown_keys(&["WEFT_RETENTION_DAYS".to_owned()]),
            ["empty"]
        );
    }
}
