//! What a logged URL keeps: no user info, fragment, secret query values or token-like path segments.

/// Query parameter names (lower case, letters and digits only) whose values are never logged.
const SECRET_NAMES: [&str; 8] = ["code", "auth", "sig", "pwd", "pass", "jwt", "hash", "otp"];
/// A parameter whose normalised name contains one of these is secret (`x-api-key`, `authToken`, …).
const SECRET_PARTS: [&str; 9] = [
    "token",
    "secret",
    "passw",
    "apikey",
    "signature",
    "credential",
    "session",
    "authorization",
    "accesskey",
];
/// Longest URL kept in a log line.
const MAX_URL_CHARS: usize = 2048;

/// Whether a query parameter's value must be hidden (`?X-Api-Key=…`, `?access_token=…`, `?key=…`).
#[must_use]
pub fn secret_param(name: &str) -> bool {
    let n: String = name
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase();
    SECRET_NAMES.contains(&n.as_str())
        || n.ends_with("key")
        || SECRET_PARTS.iter().any(|p| n.contains(p))
}

/// A path segment that looks like a credential: long, and mixing upper case, lower case and digits (chat apps'
/// webhook and bot tokens). Record IDs (numbers, hex, UUIDs) are kept.
fn secret_segment(seg: &str) -> bool {
    if seg.len() < 24 || uuid::Uuid::parse_str(seg).is_ok() {
        return false;
    }
    seg.chars().any(|c| c.is_ascii_lowercase())
        && seg.chars().any(|c| c.is_ascii_uppercase())
        && seg.chars().any(|c| c.is_ascii_digit())
}

/// The URL without user info, fragment or secret values (`?token=***`, `/services/T1/B2/***`).
#[must_use]
pub fn redact_url(raw: &str) -> String {
    redact_url_with(raw, &[])
}

/// [`redact_url`], also hiding the values of `extra` parameters (a connection's own credential parameter).
#[must_use]
pub fn redact_url_with(raw: &str, extra: &[String]) -> String {
    let Ok(mut url) = url::Url::parse(raw) else {
        return capped(unparsed(raw));
    };
    let _ = url.set_username("");
    let _ = url.set_password(None);
    url.set_fragment(None);
    if url
        .path_segments()
        .is_some_and(|mut s| s.any(secret_segment))
    {
        let path: Vec<String> = url
            .path_segments()
            .into_iter()
            .flatten()
            .map(|s| {
                if secret_segment(s) {
                    "***".to_owned()
                } else {
                    s.to_owned()
                }
            })
            .collect();
        url.set_path(&path.join("/"));
    }
    let pairs: Vec<(String, String)> = url
        .query_pairs()
        .map(|(k, v)| {
            let secret = secret_param(&k) || extra.iter().any(|e| e.eq_ignore_ascii_case(&k));
            // `***` needs no percent-encoding, so the logged URL stays readable.
            let v = if secret {
                "***".to_owned()
            } else {
                v.into_owned()
            };
            (k.into_owned(), v)
        })
        .collect();
    if pairs.is_empty() {
        url.set_query(None);
    } else {
        url.query_pairs_mut().clear().extend_pairs(pairs);
    }
    capped(url.to_string())
}

/// A URL that does not parse: no query, fragment or user info.
fn unparsed(raw: &str) -> String {
    let base = raw.split(['?', '#']).next().unwrap_or_default();
    let Some((scheme, rest)) = base.split_once("://") else {
        return base.rsplit_once('@').map_or(base, |(_, r)| r).to_owned();
    };
    let (authority, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    format!("{scheme}://{host}{path}")
}

fn capped(mut s: String) -> String {
    if s.chars().count() > MAX_URL_CHARS {
        s = s.chars().take(MAX_URL_CHARS).collect();
        s.push('…');
    }
    s
}

/// The URL as viewers see it: scheme and host only (paths and queries carry record data).
#[must_use]
pub fn origin_only(url: &str) -> String {
    url::Url::parse(url).map_or_else(|_| String::new(), |u| u.origin().ascii_serialization())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logged_urls_keep_no_credentials_or_secret_query_values() {
        assert_eq!(
            redact_url(
                "https://user:pw@api.example.com/v2/Leads/42?fields=Name&access_token=abc&KEY=k#frag"
            ),
            "https://api.example.com/v2/Leads/42?fields=Name&access_token=***&KEY=***"
        );
        assert_eq!(
            redact_url("https://api.example.com/x"),
            "https://api.example.com/x"
        );
        assert_eq!(redact_url("not a url?token=x"), "not a url");
        assert_eq!(redact_url("weird://u:p@host/x?y=1"), "weird://host/x?y=1");
        assert_eq!(unparsed("https://u:p@host:99/a@b?x"), "https://host:99/a@b");
        assert_eq!(
            origin_only("https://api.example.com/v2/Leads/42?x=1"),
            "https://api.example.com"
        );
    }

    #[test]
    fn parameter_names_are_matched_however_they_are_spelled() {
        for name in [
            "X-Api-Key",
            "apiKey",
            "authToken",
            "private_token",
            "Client-Secret",
            "sessionId",
            "key",
        ] {
            assert!(secret_param(name), "{name}");
        }
        for name in ["fields", "page", "author", "monkeys", "sort_by"] {
            assert!(!secret_param(name), "{name}");
        }
    }

    #[test]
    fn tokens_in_paths_are_hidden_and_record_ids_kept() {
        // Webhook-shaped URLs are assembled here so secret scanners do not mistake these made-up tokens for real ones.
        let token = ["aBcDeFgHiJkL", "mNoPqRsTuVw1"].concat();
        let hook = format!("https://hooks.example.com/services/T0123ABCD/B0456EFGH/{token}");
        assert_eq!(
            redact_url(&hook),
            "https://hooks.example.com/services/T0123ABCD/B0456EFGH/***"
        );
        let bot = format!("https://bots.example.com/bot123456789:{token}{token}/sendMessage");
        assert_eq!(redact_url(&bot), "https://bots.example.com/***/sendMessage");
        let ids = "https://x.example/Leads/4876876000000412001/0f8fad5b-d9cb-469f-a165-70867728950e/5f1d7b2c9e3a4b6d8c0e1f2a";
        assert_eq!(redact_url(ids), ids);
        assert!(
            redact_url(&format!("https://x.example/{}", "a".repeat(5000)))
                .chars()
                .count()
                <= MAX_URL_CHARS + 1
        );
    }
}
