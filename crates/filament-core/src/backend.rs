//! Pluggable **code** and **task** backends — crow's cross-backend model.
//!
//! crow decouples *where the code lives* (a GitHub or GitLab repository, reached
//! through `gh` / `glab`) from *where tasks are tracked* (GitHub issues or Jira,
//! reached through `gh` / `acli`). Filament mirrors that split: a session records
//! both a [`CodeProvider`] (which CLI drives PRs/CI) and a [`TaskProvider`]
//! (which CLI drives issues/tickets). The enums here are tiny, `Copy`, and serde
//! round-trip as lowercase strings so they live happily inside the session store
//! and the config file.

use serde::{Deserialize, Serialize};

/// The forge that hosts the code and its pull/merge requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, Hash)]
#[serde(rename_all = "lowercase")]
pub enum CodeProvider {
    #[default]
    GitHub,
    GitLab,
}

impl CodeProvider {
    pub const ALL: [CodeProvider; 2] = [CodeProvider::GitHub, CodeProvider::GitLab];

    /// The CLI binary Filament shells out to for this provider.
    pub fn cli(self) -> &'static str {
        match self {
            CodeProvider::GitHub => "gh",
            CodeProvider::GitLab => "glab",
        }
    }

    /// The lowercase identifier used in config / the session store.
    pub fn as_str(self) -> &'static str {
        match self {
            CodeProvider::GitHub => "github",
            CodeProvider::GitLab => "gitlab",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            CodeProvider::GitHub => "GitHub",
            CodeProvider::GitLab => "GitLab",
        }
    }

    /// What this provider calls a change request ("pull request" / "merge request").
    pub fn change_noun(self) -> &'static str {
        match self {
            CodeProvider::GitHub => "pull request",
            CodeProvider::GitLab => "merge request",
        }
    }

    pub fn parse(s: &str) -> Option<CodeProvider> {
        match s.trim().to_ascii_lowercase().as_str() {
            "github" | "gh" => Some(CodeProvider::GitHub),
            "gitlab" | "glab" => Some(CodeProvider::GitLab),
            _ => None,
        }
    }
}

impl std::fmt::Display for CodeProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// The tracker that owns issues / tickets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, Hash)]
#[serde(rename_all = "lowercase")]
pub enum TaskProvider {
    /// GitHub issues (driven by `gh`).
    #[default]
    GitHub,
    /// GitLab issues (driven by `glab`).
    GitLab,
    /// Atlassian Jira (driven by `acli`).
    Jira,
}

impl TaskProvider {
    pub const ALL: [TaskProvider; 3] = [
        TaskProvider::GitHub,
        TaskProvider::GitLab,
        TaskProvider::Jira,
    ];

    pub fn cli(self) -> &'static str {
        match self {
            TaskProvider::GitHub => "gh",
            TaskProvider::GitLab => "glab",
            TaskProvider::Jira => "acli",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            TaskProvider::GitHub => "github",
            TaskProvider::GitLab => "gitlab",
            TaskProvider::Jira => "jira",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            TaskProvider::GitHub => "GitHub Issues",
            TaskProvider::GitLab => "GitLab Issues",
            TaskProvider::Jira => "Jira",
        }
    }

    pub fn parse(s: &str) -> Option<TaskProvider> {
        match s.trim().to_ascii_lowercase().as_str() {
            "github" | "gh" => Some(TaskProvider::GitHub),
            "gitlab" | "glab" => Some(TaskProvider::GitLab),
            "jira" | "acli" => Some(TaskProvider::Jira),
            _ => None,
        }
    }
}

impl std::fmt::Display for TaskProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_provider_roundtrips() {
        for p in CodeProvider::ALL {
            assert_eq!(CodeProvider::parse(p.as_str()), Some(p));
            let json = serde_json::to_string(&p).unwrap();
            let back: CodeProvider = serde_json::from_str(&json).unwrap();
            assert_eq!(p, back);
        }
        assert_eq!(CodeProvider::parse("GH"), Some(CodeProvider::GitHub));
        assert_eq!(CodeProvider::parse("glab"), Some(CodeProvider::GitLab));
        assert_eq!(CodeProvider::parse("svn"), None);
        assert_eq!(CodeProvider::default(), CodeProvider::GitHub);
    }

    #[test]
    fn task_provider_roundtrips() {
        for p in TaskProvider::ALL {
            assert_eq!(TaskProvider::parse(p.as_str()), Some(p));
            let json = serde_json::to_string(&p).unwrap();
            let back: TaskProvider = serde_json::from_str(&json).unwrap();
            assert_eq!(p, back);
        }
        assert_eq!(TaskProvider::parse("acli"), Some(TaskProvider::Jira));
        assert_eq!(TaskProvider::Jira.cli(), "acli");
    }
}
