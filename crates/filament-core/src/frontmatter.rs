//! Hand-rolled YAML frontmatter splitter.
//!
//! We deliberately do *not* use an off-the-shelf frontmatter crate: lossless
//! editing needs the exact byte spans of the frontmatter and body so we can
//! rewrite individual keys without disturbing the rest of the file. The returned
//! [`Split`] holds byte ranges into the original string — no copies.
//!
//! Recognised shape (the standard Markdown frontmatter convention):
//! ```text
//! ---
//! key: value      <- frontmatter span (between the fences, excluding them)
//! ---
//! body text...    <- body span (everything after the closing fence line)
//! ```
//! A leading UTF-8 BOM is tolerated. The opening fence must be the very first
//! line; the closing fence is a line equal to `---` or `...`.

use std::ops::Range;

/// Byte spans for the frontmatter and body of a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Split {
    /// Whether a well-formed `---` … `---` frontmatter block was found.
    pub has_frontmatter: bool,
    /// Byte range of the frontmatter *content* (between the fences). Empty when
    /// `has_frontmatter` is false.
    pub frontmatter: Range<usize>,
    /// Byte range of the body (everything after the closing fence line).
    pub body: Range<usize>,
}

/// Split `text` into frontmatter and body byte ranges.
pub fn split_frontmatter(text: &str) -> Split {
    let bom = if text.starts_with('\u{feff}') {
        '\u{feff}'.len_utf8()
    } else {
        0
    };

    // The opening fence must be the first line.
    let Some((first_line, after_first)) = next_line(text, bom) else {
        return no_frontmatter(text, bom);
    };
    if !is_fence(first_line) {
        return no_frontmatter(text, bom);
    }

    // Scan for the closing fence.
    let fm_start = after_first;
    let mut cursor = after_first;
    while let Some((line, after_line)) = next_line(text, cursor) {
        if is_close_fence(line) {
            return Split {
                has_frontmatter: true,
                frontmatter: fm_start..cursor,
                body: after_line..text.len(),
            };
        }
        cursor = after_line;
    }

    // Opening fence but no closing fence => malformed; treat as bodyless raw.
    no_frontmatter(text, bom)
}

fn no_frontmatter(text: &str, bom: usize) -> Split {
    Split {
        has_frontmatter: false,
        frontmatter: 0..0,
        body: bom..text.len(),
    }
}

/// Returns the next line (without its trailing newline) starting at `start`,
/// plus the byte offset just past the line's newline (or end of string).
fn next_line(text: &str, start: usize) -> Option<(&str, usize)> {
    if start >= text.len() {
        return None;
    }
    let rest = &text[start..];
    match rest.find('\n') {
        Some(pos) => Some((&text[start..start + pos], start + pos + 1)),
        None => Some((&text[start..], text.len())),
    }
}

/// An opening fence: a line equal to `---` (ignoring trailing CR/space).
fn is_fence(line: &str) -> bool {
    line.trim_end() == "---"
}

/// A closing fence: a line equal to `---` or `...`.
fn is_close_fence(line: &str) -> bool {
    matches!(line.trim_end(), "---" | "...")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(text: &str) -> (bool, &str, &str) {
        let s = split_frontmatter(text);
        (
            s.has_frontmatter,
            &text[s.frontmatter.clone()],
            &text[s.body.clone()],
        )
    }

    #[test]
    fn basic() {
        let t = "---\nname: a\n---\nbody\n";
        assert_eq!(parts(t), (true, "name: a\n", "body\n"));
    }

    #[test]
    fn crlf_fences() {
        let t = "---\r\nname: a\r\n---\r\nbody\r\n";
        let (has, fm, body) = parts(t);
        assert!(has);
        assert_eq!(fm, "name: a\r\n");
        assert_eq!(body, "body\r\n");
    }

    #[test]
    fn dotdotdot_close() {
        let t = "---\nname: a\n...\nbody";
        assert_eq!(parts(t), (true, "name: a\n", "body"));
    }

    #[test]
    fn bom_prefix() {
        let t = "\u{feff}---\nname: a\n---\nbody";
        assert!(parts(t).0);
        assert_eq!(parts(t).1, "name: a\n");
    }

    #[test]
    fn no_frontmatter_plain() {
        let t = "just a body\nwith lines\n";
        assert_eq!(parts(t), (false, "", "just a body\nwith lines\n"));
    }

    #[test]
    fn unterminated_is_not_frontmatter() {
        let t = "---\nname: a\nno close";
        assert!(!parts(t).0);
    }

    #[test]
    fn empty_frontmatter() {
        let t = "---\n---\nbody";
        assert_eq!(parts(t), (true, "", "body"));
    }
}
