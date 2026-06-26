//! The agent frontmatter schema.
//!
//! Only `name` and `description` are required; everything else is optional.
//! Unknown keys are tolerated (we never reject a file for having a field we
//! don't model). Several keys appear in multiple casings in the wild, handled
//! with `rename` + `alias`.

use serde::Deserialize;
use serde_norway::Value as Yaml;

use super::enums::{AgentColor, Effort, Isolation, Memory, PermissionMode};
use super::model_id::ModelChoice;
use super::tools::ToolList;
use crate::scope::Scope;

#[derive(Debug, Clone, Deserialize)]
pub struct AgentFrontmatter {
    pub name: String,
    pub description: String,

    #[serde(default)]
    pub tools: Option<ToolList>,
    #[serde(default, rename = "disallowedTools", alias = "disallowed-tools")]
    pub disallowed_tools: Option<ToolList>,

    #[serde(default)]
    pub model: ModelChoice,
    #[serde(default, rename = "permissionMode", alias = "permission-mode")]
    pub permission_mode: Option<PermissionMode>,
    #[serde(default, rename = "maxTurns", alias = "max-turns")]
    pub max_turns: Option<u32>,

    #[serde(default)]
    pub skills: Vec<String>,
    /// A list of server names or an inline server map — kept opaque for lossless
    /// display/round-trip.
    #[serde(default, rename = "mcpServers", alias = "mcp-servers")]
    pub mcp_servers: Option<Yaml>,
    /// Per-agent lifecycle hooks — kept opaque.
    #[serde(default)]
    pub hooks: Option<Yaml>,

    #[serde(default)]
    pub memory: Option<Memory>,
    #[serde(default)]
    pub background: Option<bool>,
    #[serde(default)]
    pub effort: Option<Effort>,
    #[serde(default)]
    pub isolation: Option<Isolation>,
    #[serde(default)]
    pub color: Option<AgentColor>,

    #[serde(default, rename = "initialPrompt", alias = "initial-prompt")]
    pub initial_prompt: Option<String>,
    #[serde(default, alias = "whenToUse", alias = "when-to-use")]
    pub when_to_use: Option<String>,
}

impl AgentFrontmatter {
    /// The effective description shown to the model = `description` plus an
    /// appended `when_to_use`, if present.
    pub fn effective_description(&self) -> String {
        match &self.when_to_use {
            Some(w) if !w.trim().is_empty() => format!("{}\n\n{}", self.description, w),
            _ => self.description.clone(),
        }
    }

    /// A minimal template for the creation wizard.
    pub fn template(name: &str) -> String {
        format!(
            "---\nname: {name}\ndescription: Describe when Claude should delegate to this agent.\ntools: Read, Grep, Glob\nmodel: inherit\ncolor: blue\n---\n\nYou are {name}. Describe the agent's role and instructions here.\n"
        )
    }
}

/// A fully-loaded agent: parsed frontmatter, the markdown body (system prompt),
/// and the scope it came from. The raw text needed for lossless editing lives on
/// the owning [`super::Entry`].
#[derive(Debug, Clone)]
pub struct Agent {
    pub fm: AgentFrontmatter,
    pub body: String,
    pub scope: Scope,
}
