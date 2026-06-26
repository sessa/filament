//! The unified data model the UI renders.
//!
//! Every discovered file becomes an [`Entry`] tagged with its [`ItemKind`],
//! [`Scope`], a numeric [`Entry::precedence`], and a [`Payload`] that is either
//! the typed parse result or [`Payload::Invalid`]. A [`Catalog`] owns all
//! entries and, via [`Catalog::resolve`], works out which definition *wins* when
//! several share a name and marks the rest as shadowed.

mod agent;
mod command;
mod enums;
mod hook;
mod mcp;
mod model_id;
mod settings;
mod skill;
mod tools;

use std::ops::Range;
use std::path::{Path, PathBuf};

pub use agent::{Agent, AgentFrontmatter};
pub use command::CommandFrontmatter;
pub use enums::{AgentColor, Effort, Isolation, Memory, PermissionMode};
pub use hook::{parse_hooks, HookCommand, HookEventGroup, HookMatcher, HOOK_EVENTS};
pub use mcp::{McpServer, McpTransport};
pub use model_id::ModelChoice;
pub use settings::{Permissions, Settings};
pub use skill::SkillFrontmatter;
pub use tools::{split_tool_tokens, ToolList, ToolSpec};

use crate::error::ParseError;
use crate::scope::Scope;

/// The kind of configuration item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ItemKind {
    Agent,
    Skill,
    Command,
    McpServer,
    Settings,
}

impl ItemKind {
    pub const ALL: [ItemKind; 5] = [
        ItemKind::Agent,
        ItemKind::Skill,
        ItemKind::Command,
        ItemKind::McpServer,
        ItemKind::Settings,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ItemKind::Agent => "Agent",
            ItemKind::Skill => "Skill",
            ItemKind::Command => "Command",
            ItemKind::McpServer => "MCP Server",
            ItemKind::Settings => "Settings",
        }
    }

    /// Plural label for sidebar group headers.
    pub fn plural(self) -> &'static str {
        match self {
            ItemKind::Agent => "Agents",
            ItemKind::Skill => "Skills",
            ItemKind::Command => "Commands",
            ItemKind::McpServer => "MCP Servers",
            ItemKind::Settings => "Settings",
        }
    }

    /// Whether items of this kind are resolved by `name` (and thus can shadow
    /// each other). Settings layer rather than shadow.
    pub fn resolves_by_name(self) -> bool {
        !matches!(self, ItemKind::Settings)
    }
}

impl std::fmt::Display for ItemKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// A stable identity for an entry, derived from its source path (plus the server
/// name for MCP entries, since one file holds many). Stable across rescans so
/// the UI can preserve the selection.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ItemId(pub String);

impl ItemId {
    pub fn for_path(kind: ItemKind, path: &Path) -> ItemId {
        ItemId(format!("{}:{}", kind.label(), path.display()))
    }

    pub fn for_mcp(path: &Path, server: &str) -> ItemId {
        ItemId(format!("mcp:{}::{server}", path.display()))
    }
}

impl std::fmt::Display for ItemId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The original file text plus byte spans, used for lossless editing. Present
/// for Markdown-backed items.
#[derive(Debug, Clone)]
pub struct RawDoc {
    pub source_path: PathBuf,
    pub raw_text: String,
    pub frontmatter: Range<usize>,
    pub body: Range<usize>,
    pub has_frontmatter: bool,
}

impl RawDoc {
    pub fn frontmatter_str(&self) -> &str {
        &self.raw_text[self.frontmatter.clone()]
    }

    pub fn body_str(&self) -> &str {
        &self.raw_text[self.body.clone()]
    }
}

/// The typed parse result for an entry, or an error if the file was malformed.
#[derive(Debug, Clone)]
pub enum Payload {
    Agent(AgentFrontmatter),
    Skill(SkillFrontmatter),
    Command(CommandFrontmatter),
    Mcp(McpServer),
    Settings(Settings),
    Invalid(ParseError),
}

impl Payload {
    pub fn is_valid(&self) -> bool {
        !matches!(self, Payload::Invalid(_))
    }

    pub fn error(&self) -> Option<&ParseError> {
        match self {
            Payload::Invalid(e) => Some(e),
            _ => None,
        }
    }
}

/// One configuration item.
#[derive(Debug, Clone)]
pub struct Entry {
    pub id: ItemId,
    pub kind: ItemKind,
    /// Display name (frontmatter `name`, MCP server name, or filename fallback).
    pub name: String,
    pub scope: Scope,
    /// Lower wins. Encodes scope rank *and* within-scope order (nearest project
    /// `.claude` first).
    pub precedence: u32,
    pub source_path: PathBuf,
    /// Present for Markdown-backed items (agents/skills/commands).
    pub raw: Option<RawDoc>,
    pub payload: Payload,

    /// Resolution results, filled by [`Catalog::resolve`].
    pub winning: bool,
    pub shadowed_by: Option<ItemId>,
    pub shadows: Vec<ItemId>,
}

impl Entry {
    /// The markdown body (system prompt / command prompt), if any.
    pub fn body(&self) -> Option<&str> {
        self.raw.as_ref().map(RawDoc::body_str)
    }

    /// A short one-line description for the sidebar, when available.
    pub fn description(&self) -> Option<&str> {
        match &self.payload {
            Payload::Agent(a) => Some(a.description.as_str()),
            Payload::Skill(s) => Some(s.description.as_str()),
            Payload::Command(c) => c.description.as_deref(),
            Payload::Mcp(m) => Some(m.transport.kind()),
            Payload::Settings(_) => None,
            Payload::Invalid(_) => None,
        }
    }

    /// The agent color, if this is an agent that declares one.
    pub fn color(&self) -> Option<AgentColor> {
        match &self.payload {
            Payload::Agent(a) => a.color,
            _ => None,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.payload.is_valid()
    }
}

/// All discovered entries plus collision resolution.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    pub entries: Vec<Entry>,
}

impl Catalog {
    /// Build a catalog from raw entries and resolve precedence/shadowing.
    pub fn new(mut entries: Vec<Entry>) -> Catalog {
        Self::resolve(&mut entries);
        Catalog { entries }
    }

    /// Mark the winning entry in each `(kind, name)` group and record shadowing.
    fn resolve(entries: &mut [Entry]) {
        use std::collections::HashMap;

        // Group indices by (kind, name) for name-resolved kinds.
        let mut groups: HashMap<(ItemKind, String), Vec<usize>> = HashMap::new();
        for (i, e) in entries.iter().enumerate() {
            if e.kind.resolves_by_name() && e.is_valid() {
                groups.entry((e.kind, e.name.clone())).or_default().push(i);
            } else {
                // Settings and invalid entries always "win" (nothing to resolve).
                // (Set below by default; recorded here for clarity.)
            }
        }

        // Default everyone to winning; we'll demote shadowed ones.
        for e in entries.iter_mut() {
            e.winning = true;
            e.shadowed_by = None;
            e.shadows.clear();
        }

        for indices in groups.values() {
            if indices.len() < 2 {
                continue;
            }
            // Sort by precedence ascending; ties broken by path for determinism.
            let mut ordered = indices.clone();
            ordered.sort_by(|&a, &b| {
                entries[a]
                    .precedence
                    .cmp(&entries[b].precedence)
                    .then_with(|| entries[a].source_path.cmp(&entries[b].source_path))
            });
            let winner = ordered[0];
            let winner_id = entries[winner].id.clone();
            for &idx in &ordered[1..] {
                entries[idx].winning = false;
                entries[idx].shadowed_by = Some(winner_id.clone());
                let shadowed_id = entries[idx].id.clone();
                entries[winner].shadows.push(shadowed_id);
            }
        }
    }

    pub fn get(&self, id: &ItemId) -> Option<&Entry> {
        self.entries.iter().find(|e| &e.id == id)
    }

    pub fn by_kind(&self, kind: ItemKind) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(move |e| e.kind == kind)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Count of entries that failed to parse.
    pub fn invalid_count(&self) -> usize {
        self.entries.iter().filter(|e| !e.is_valid()).count()
    }
}
