//! Small closed enums used across the frontmatter schema. Each carries the
//! exact on-disk spelling (Claude Code mixes lowercase and camelCase) plus
//! `all()`/`label()` helpers the UI reuses for dropdowns and chips.

use serde::{Deserialize, Serialize};

/// `permissionMode` — note the camelCase spellings on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PermissionMode {
    #[serde(rename = "default")]
    Default,
    #[serde(rename = "acceptEdits")]
    AcceptEdits,
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "dontAsk")]
    DontAsk,
    #[serde(rename = "bypassPermissions")]
    BypassPermissions,
    #[serde(rename = "plan")]
    Plan,
}

impl PermissionMode {
    pub const ALL: [PermissionMode; 6] = [
        PermissionMode::Default,
        PermissionMode::AcceptEdits,
        PermissionMode::Auto,
        PermissionMode::DontAsk,
        PermissionMode::BypassPermissions,
        PermissionMode::Plan,
    ];

    /// The literal as written in YAML.
    pub fn as_str(self) -> &'static str {
        match self {
            PermissionMode::Default => "default",
            PermissionMode::AcceptEdits => "acceptEdits",
            PermissionMode::Auto => "auto",
            PermissionMode::DontAsk => "dontAsk",
            PermissionMode::BypassPermissions => "bypassPermissions",
            PermissionMode::Plan => "plan",
        }
    }
}

impl std::fmt::Display for PermissionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// `effort` — reasoning effort override.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Effort {
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

impl Effort {
    pub const ALL: [Effort; 5] = [
        Effort::Low,
        Effort::Medium,
        Effort::High,
        Effort::Xhigh,
        Effort::Max,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Effort::Low => "low",
            Effort::Medium => "medium",
            Effort::High => "high",
            Effort::Xhigh => "xhigh",
            Effort::Max => "max",
        }
    }
}

impl std::fmt::Display for Effort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// `color` — the eight display colors Claude Code recognises for agents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentColor {
    Red,
    Blue,
    Green,
    Yellow,
    Purple,
    Orange,
    Pink,
    Cyan,
}

impl AgentColor {
    pub const ALL: [AgentColor; 8] = [
        AgentColor::Red,
        AgentColor::Blue,
        AgentColor::Green,
        AgentColor::Yellow,
        AgentColor::Purple,
        AgentColor::Orange,
        AgentColor::Pink,
        AgentColor::Cyan,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            AgentColor::Red => "red",
            AgentColor::Blue => "blue",
            AgentColor::Green => "green",
            AgentColor::Yellow => "yellow",
            AgentColor::Purple => "purple",
            AgentColor::Orange => "orange",
            AgentColor::Pink => "pink",
            AgentColor::Cyan => "cyan",
        }
    }

    /// A representative sRGB triple. The UI maps these per-theme for legibility,
    /// but this gives a sensible default swatch.
    pub fn rgb(self) -> (u8, u8, u8) {
        match self {
            AgentColor::Red => (0xE5, 0x48, 0x4D),
            AgentColor::Blue => (0x4C, 0x8B, 0xF5),
            AgentColor::Green => (0x3F, 0xB9, 0x50),
            AgentColor::Yellow => (0xE8, 0xC3, 0x4A),
            AgentColor::Purple => (0x9B, 0x59, 0xD6),
            AgentColor::Orange => (0xE8, 0x8B, 0x3C),
            AgentColor::Pink => (0xE5, 0x6B, 0xAE),
            AgentColor::Cyan => (0x3F, 0xC1, 0xC9),
        }
    }
}

impl std::fmt::Display for AgentColor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// `memory` — where persistent agent memory is stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Memory {
    User,
    Project,
    Local,
}

impl Memory {
    pub const ALL: [Memory; 3] = [Memory::User, Memory::Project, Memory::Local];

    pub fn as_str(self) -> &'static str {
        match self {
            Memory::User => "user",
            Memory::Project => "project",
            Memory::Local => "local",
        }
    }
}

impl std::fmt::Display for Memory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// `isolation` — currently only `worktree`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Isolation {
    Worktree,
}

impl Isolation {
    pub const ALL: [Isolation; 1] = [Isolation::Worktree];

    pub fn as_str(self) -> &'static str {
        match self {
            Isolation::Worktree => "worktree",
        }
    }
}

impl std::fmt::Display for Isolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
