//! Configuration scope and precedence.
//!
//! Claude Code resolves same-named definitions by scope. Variant order here is
//! the precedence order — `Managed` wins over `Project`, which wins over `User`,
//! which wins over `Plugin` — so the derived `Ord` ranks "more authoritative"
//! as *smaller*. Fine-grained precedence (e.g. nearest project `.claude` wins)
//! is handled with a numeric index assigned at discovery time; see
//! [`crate::model::Entry::precedence`].

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    /// Enterprise-managed settings deployed by an organization. Highest precedence.
    Managed,
    /// Committed to a repository under `.claude/` and shared with the team.
    Project,
    /// The user's personal config under `~/.claude/`.
    User,
    /// Provided by an installed plugin. Lowest precedence.
    Plugin,
}

impl Scope {
    /// Short human label for chips/badges.
    pub fn label(self) -> &'static str {
        match self {
            Scope::Managed => "Managed",
            Scope::Project => "Project",
            Scope::User => "User",
            Scope::Plugin => "Plugin",
        }
    }

    pub const ALL: [Scope; 4] = [Scope::Managed, Scope::Project, Scope::User, Scope::Plugin];
}

impl std::fmt::Display for Scope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}
