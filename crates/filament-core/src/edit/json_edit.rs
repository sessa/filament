//! JSON config editing (`.mcp.json`, `settings.json`).
//!
//! Unlike Markdown frontmatter, JSON files are re-serialized from a parsed
//! [`serde_json::Value`]. We keep the whole document as a `Value`, apply edits to
//! specific paths, and pretty-print with stable 2-space indentation. The
//! structured form editor in the UI builds on these primitives (M5).

use serde_json::Value as Json;

use crate::error::ParseError;

/// Parse a JSON document, preserving the full structure for editing.
pub fn parse_document(path: &std::path::Path, text: &str) -> Result<Json, ParseError> {
    serde_json::from_str(text).map_err(|e| ParseError::Json {
        path: path.to_path_buf(),
        message: e.to_string(),
    })
}

/// Pretty-print a JSON value with a trailing newline (editor-friendly).
pub fn to_pretty(value: &Json) -> String {
    let mut s = serde_json::to_string_pretty(value).unwrap_or_default();
    s.push('\n');
    s
}

/// Set a top-level key on a JSON object document, returning the updated text.
/// Creates the object if the document is empty/null.
pub fn set_top_level(doc: &Json, key: &str, value: Json) -> Json {
    let mut obj = doc.as_object().cloned().unwrap_or_default();
    obj.insert(key.to_string(), value);
    Json::Object(obj)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_pretty() {
        let v: Json = serde_json::json!({"permissions": {"allow": ["Skill"]}});
        let text = to_pretty(&v);
        assert!(text.ends_with("}\n"));
        let back: Json = serde_json::from_str(&text).unwrap();
        assert_eq!(back, v);
    }

    #[test]
    fn set_key() {
        let doc = serde_json::json!({"a": 1});
        let out = set_top_level(&doc, "b", serde_json::json!(2));
        assert_eq!(out, serde_json::json!({"a": 1, "b": 2}));
    }
}
