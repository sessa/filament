//! Lossless editing and atomic persistence.
//!
//! The guiding rule: never re-serialize a whole Markdown file from the typed
//! struct (that would drop comments, key order, and formatting). Instead rewrite
//! only the touched frontmatter keys ([`frontmatter_edit`]) and swap the body
//! wholesale, then write atomically ([`atomic_write`]). JSON files
//! ([`json_edit`]) are the exception — they round-trip through `serde_json`.

pub mod atomic_write;
pub mod frontmatter_edit;
pub mod json_edit;

pub use atomic_write::atomic_write;
pub use frontmatter_edit::{remove_key, replace_body, set_scalar};
