//! Surgical, formatting-preserving edits to a frontmatter block.
//!
//! These operate on the original file text plus the frontmatter/body byte spans
//! from [`crate::frontmatter::split_frontmatter`]. Only the targeted top-level
//! scalar key (or the body) is rewritten; everything else — comments, key order,
//! blank lines, indentation of unrelated keys — is preserved byte-for-byte.
//!
//! Scope: top-level *scalar* keys (the common case: `name`, `description`,
//! `model`, `color`, `effort`, …). Block-valued keys (`hooks`, nested
//! `mcpServers`) are out of scope here and handled as opaque text.

use std::ops::Range;

/// Set a top-level scalar `key` to `value`, replacing an existing line or
/// appending a new one at the end of the frontmatter.
pub fn set_scalar(raw: &str, frontmatter: Range<usize>, key: &str, value: &str) -> String {
    let fm = &raw[frontmatter.clone()];
    let mut new_fm = String::with_capacity(fm.len() + key.len() + value.len() + 4);
    let mut found = false;

    for line in fm.split_inclusive('\n') {
        if !found && is_top_level_key(line, key) {
            let newline = if line.ends_with('\n') { "\n" } else { "" };
            new_fm.push_str(&format!("{key}: {value}{newline}"));
            found = true;
        } else {
            new_fm.push_str(line);
        }
    }

    if !found {
        if !new_fm.is_empty() && !new_fm.ends_with('\n') {
            new_fm.push('\n');
        }
        new_fm.push_str(&format!("{key}: {value}\n"));
    }

    splice(raw, frontmatter, &new_fm)
}

/// Remove a top-level scalar `key` entirely (no-op if absent).
pub fn remove_key(raw: &str, frontmatter: Range<usize>, key: &str) -> String {
    let fm = &raw[frontmatter.clone()];
    let mut new_fm = String::with_capacity(fm.len());
    let mut removed = false;
    for line in fm.split_inclusive('\n') {
        if !removed && is_top_level_key(line, key) {
            removed = true; // skip this line
        } else {
            new_fm.push_str(line);
        }
    }
    splice(raw, frontmatter, &new_fm)
}

/// Replace the body (everything after the closing fence) wholesale.
pub fn replace_body(raw: &str, body: Range<usize>, new_body: &str) -> String {
    let mut out = String::with_capacity(body.start + new_body.len());
    out.push_str(&raw[..body.start]);
    out.push_str(new_body);
    out
}

fn splice(raw: &str, span: Range<usize>, replacement: &str) -> String {
    let mut out = String::with_capacity(raw.len() - span.len() + replacement.len());
    out.push_str(&raw[..span.start]);
    out.push_str(replacement);
    out.push_str(&raw[span.end..]);
    out
}

/// Whether `line` is a top-level (unindented) mapping for `key`, i.e. matches
/// `^key\s*:`.
fn is_top_level_key(line: &str, key: &str) -> bool {
    if line.starts_with([' ', '\t']) {
        return false;
    }
    match line.trim_end().strip_prefix(key) {
        Some(rest) => rest.trim_start().starts_with(':'),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontmatter::split_frontmatter;

    fn fm_span(text: &str) -> Range<usize> {
        split_frontmatter(text).frontmatter
    }

    #[test]
    fn replaces_existing_scalar_preserving_rest() {
        let raw =
            "---\nname: a\nmodel: sonnet # keep comment? value replaced\ncolor: red\n---\nbody\n";
        let out = set_scalar(raw, fm_span(raw), "model", "opus");
        assert!(out.contains("model: opus\n"));
        assert!(out.contains("name: a\n"));
        assert!(out.contains("color: red\n"));
        assert!(out.ends_with("---\nbody\n"));
    }

    #[test]
    fn appends_missing_scalar() {
        let raw = "---\nname: a\n---\nbody\n";
        let out = set_scalar(raw, fm_span(raw), "color", "green");
        assert!(out.contains("name: a\n"));
        assert!(out.contains("color: green\n"));
    }

    #[test]
    fn removes_key() {
        let raw = "---\nname: a\ncolor: red\n---\nbody\n";
        let out = remove_key(raw, fm_span(raw), "color");
        assert!(!out.contains("color"));
        assert!(out.contains("name: a\n"));
    }

    #[test]
    fn does_not_match_substring_keys() {
        let raw = "---\nmodelx: keep\nmodel: sonnet\n---\nb";
        let out = set_scalar(raw, fm_span(raw), "model", "opus");
        assert!(out.contains("modelx: keep\n"));
        assert!(out.contains("model: opus\n"));
    }

    #[test]
    fn replace_body_only() {
        let raw = "---\nname: a\n---\nold body\n";
        let split = split_frontmatter(raw);
        let out = replace_body(raw, split.body, "new body\n");
        assert_eq!(out, "---\nname: a\n---\nnew body\n");
    }
}
