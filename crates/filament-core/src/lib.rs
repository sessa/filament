//! `filament-core` — UI-free engine for discovering, parsing, and losslessly
//! editing Claude Code configuration.
//!
//! The crate has **no UI dependencies** so all parsing / discovery / precedence /
//! round-trip logic is fast to compile and unit-testable headlessly.
//!
//! Pipeline: [`discovery::discover`] walks the configured roots, parses every
//! file into an [`model::Entry`] (capturing per-file errors as diagnostics rather
//! than aborting), and resolves name collisions across [`scope::Scope`]s into a
//! [`model::Catalog`]. Edits go through [`edit`], which rewrites only the changed
//! frontmatter keys and writes atomically.

pub mod discovery;
pub mod edit;
pub mod error;
pub mod frontmatter;
pub mod model;
pub mod parse;
pub mod scope;
pub mod validate;
pub mod workspace;

pub use error::{CoreError, ParseError};
pub use model::{
    parse_hooks, Agent, AgentColor, AgentFrontmatter, Catalog, CommandFrontmatter, Effort, Entry,
    HookCommand, HookEventGroup, HookMatcher, Isolation, ItemId, ItemKind, McpServer, McpTransport,
    Memory, ModelChoice, Payload, PermissionMode, Permissions, RawDoc, Settings, SkillFrontmatter,
    ToolList, ToolSpec,
};
pub use scope::Scope;
pub use validate::{validate_agent, validate_skill, ValidationReport};
pub use workspace::{DiscoveryOptions, Workspace};
