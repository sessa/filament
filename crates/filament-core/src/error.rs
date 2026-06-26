//! Error and diagnostic types.
//!
//! [`CoreError`] is for operations that genuinely fail (I/O during a save). The
//! discovery/parse path never aborts on a bad file — instead it records a
//! [`ParseError`] on the affected [`crate::model::Entry`] so the UI can show an
//! error badge while still listing everything else.

use std::path::{Path, PathBuf};

use thiserror::Error;

/// A hard failure from a core operation (mostly I/O during read/write).
#[derive(Debug, Error)]
pub enum CoreError {
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(transparent)]
    Parse(#[from] ParseError),

    #[error("{0}")]
    Other(String),
}

impl CoreError {
    pub fn io(path: impl AsRef<Path>, source: std::io::Error) -> Self {
        CoreError::Io {
            path: path.as_ref().to_path_buf(),
            source,
        }
    }
}

/// A per-file parse problem. Carried on an [`crate::model::Entry`] rather than
/// bubbled up, so one malformed file doesn't hide the rest of the config.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ParseError {
    #[error("invalid YAML frontmatter: {message}")]
    Yaml { path: PathBuf, message: String },

    #[error("invalid JSON: {message}")]
    Json { path: PathBuf, message: String },

    #[error("missing frontmatter (file must start with a `---` line)")]
    MissingFrontmatter { path: PathBuf },

    #[error("missing required field: {field}")]
    MissingField { path: PathBuf, field: String },

    #[error("could not read file: {message}")]
    Unreadable { path: PathBuf, message: String },

    #[error("{message}")]
    Other { path: PathBuf, message: String },
}

impl ParseError {
    /// The file this error refers to.
    pub fn path(&self) -> &Path {
        match self {
            ParseError::Yaml { path, .. }
            | ParseError::Json { path, .. }
            | ParseError::MissingFrontmatter { path }
            | ParseError::MissingField { path, .. }
            | ParseError::Unreadable { path, .. }
            | ParseError::Other { path, .. } => path,
        }
    }
}
