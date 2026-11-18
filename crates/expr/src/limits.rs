//! Limits on templates written by users (any signed-in person can create an org and a flow): the source size, the
//! block tags allowed (no `set`, macros or `namespace`, so nothing builds up state across loop iterations), and the
//! size of rendered text. Large values built inside one expression are not bounded here (minijinja has no memory
//! hook); see `docs/STATUS.md`.

use std::io::{self, Write};

/// Longest template string accepted (one string leaf of an input).
pub const MAX_TEMPLATE_BYTES: usize = 64 * 1024;
/// Most text a template may render.
pub const MAX_RENDERED_BYTES: usize = 10 * 1024 * 1024;

/// Block tags a template may use (`{% … %}`): conditions and loops only.
const ALLOWED_TAGS: &[&str] = &[
    "if", "elif", "else", "endif", "for", "endfor", "break", "continue", "raw", "endraw",
];

/// Checks the source size and block tags of a template.
pub fn check_source(s: &str) -> Result<(), String> {
    if s.len() > MAX_TEMPLATE_BYTES {
        return Err(format!(
            "the template is {} KB; the most is {} KB",
            s.len() / 1024,
            MAX_TEMPLATE_BYTES / 1024
        ));
    }
    let mut rest = s;
    while let Some(at) = rest.find("{%") {
        let tag_start = rest[at + 2..]
            .trim_start_matches(['-', '+', '~'])
            .trim_start();
        let tag: String = tag_start
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if !ALLOWED_TAGS.contains(&tag.as_str()) {
            return Err(format!(
                "`{{% {tag} %}}` is not supported: use `if` and `for` blocks, and `{{{{ … }}}}` for values"
            ));
        }
        rest = &tag_start[tag.len()..];
        if tag == "raw" {
            // Text inside raw is not template code.
            match rest.find("endraw") {
                Some(end) => rest = &rest[end..],
                None => return Ok(()),
            }
        }
    }
    Ok(())
}

/// A buffer that refuses to grow past `MAX_RENDERED_BYTES`.
#[derive(Debug, Default)]
pub struct CappedWriter {
    pub buf: Vec<u8>,
    pub overflowed: bool,
}

impl Write for CappedWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        if self.buf.len() + data.len() > MAX_RENDERED_BYTES {
            self.overflowed = true;
            return Err(io::Error::other("rendered text too large"));
        }
        self.buf.extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_conditions_and_loops_only() {
        assert!(check_source("{% if a %}x{% else %}y{% endif %}").is_ok());
        assert!(check_source("{%- for i in items -%}{{ i }}{% endfor %}").is_ok());
        assert!(check_source("{% raw %}{% set x = 1 %}{% endraw %}").is_ok());
        for bad in [
            "{% set a = 'x' %}",
            "{% macro m() %}{% endmacro %}",
            "{%- filter upper %}x{% endfilter %}",
            "{% include 'x' %}",
        ] {
            assert!(check_source(bad).is_err(), "{bad}");
        }
        assert!(check_source(&"x".repeat(MAX_TEMPLATE_BYTES + 1)).is_err());
    }

    #[test]
    fn the_writer_stops_at_the_cap() {
        let mut w = CappedWriter::default();
        assert!(w.write_all(&vec![b'x'; MAX_RENDERED_BYTES]).is_ok());
        assert!(w.write_all(b"y").is_err());
        assert!(w.overflowed);
    }
}
