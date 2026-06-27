//! Workspace configuration — crow's `config.json`, ported.
//!
//! This is the cross-backend / automation configuration: which forge and tracker
//! to use by default, the branch prefix, per-workspace overrides, the automation
//! toggles, repo exclude lists (with `*` wildcards), the poll interval, and Jira
//! settings. It's persisted as JSON in the OS data directory (separate from the
//! appearance-only [`crate::session::SessionStore`] and the UI preferences) and
//! is fully UI-free / unit-tested.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::backend::{CodeProvider, TaskProvider};

/// The automation suite (Settings → Automation in crow). Every cross-cutting
/// toggle lives here; defaults mirror crow (most off, manager-auto on).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Automation {
    /// Auto-create a session when an assigned issue carries [`Self::auto_label`].
    pub auto_create: bool,
    /// The label that opts an issue into auto-creation (crow: `crow:auto`).
    pub auto_label: String,
    /// Suggest opening a PR when a session has work but no linked PR.
    pub suggest_pr: bool,
    /// Auto-start a review session when a PR becomes reviewable.
    pub auto_start_review: bool,
    /// Type a follow-up into the session terminal when a review requests changes.
    pub respond_changes_requested: bool,
    /// Type a follow-up into the session terminal when CI fails.
    pub respond_failed_ci: bool,
    /// Squash-merge PRs that carry [`Self::merge_label`] once green & approved.
    pub auto_merge: bool,
    /// The label that opts a PR into auto-merge (crow: `crow:merge`).
    pub merge_label: String,
    /// Move sessions to Done when their PR merges / issue closes (with evidence).
    pub auto_complete: bool,
    /// Launch the manager terminal with `--permission-mode auto`.
    pub manager_auto_permission: bool,
    /// Launch sessions/manager with `--rc` for claude.ai / mobile remote control.
    pub remote_control: bool,
}

impl Default for Automation {
    fn default() -> Self {
        Automation {
            auto_create: false,
            auto_label: "crow:auto".to_string(),
            suggest_pr: false,
            auto_start_review: false,
            respond_changes_requested: false,
            respond_failed_ci: false,
            auto_merge: false,
            merge_label: "crow:merge".to_string(),
            auto_complete: true,
            manager_auto_permission: true,
            remote_control: false,
        }
    }
}

/// Jira (task backend) settings — used when [`TaskProvider::Jira`] is active.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct JiraConfig {
    /// e.g. `https://acme.atlassian.net`.
    pub site_url: String,
    /// e.g. `ACME` — the project key new tickets / queries are scoped to.
    pub project_key: String,
    /// Maps pipeline state names (`Ready`, `In Progress`, …) to this project's
    /// Jira status names. Missing entries fall back to the pipeline name.
    pub status_map: BTreeMap<String, String>,
}

impl JiraConfig {
    /// The Jira status name for a pipeline state (falls back to `pipeline`).
    pub fn status_for(&self, pipeline: &str) -> String {
        self.status_map
            .get(pipeline)
            .cloned()
            .unwrap_or_else(|| pipeline.to_string())
    }
}

/// A named workspace that can override the global defaults (crow's per-workspace
/// `provider` / `cli` / `host` / `branchPrefix`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct WorkspaceConfig {
    pub name: String,
    pub provider: CodeProvider,
    pub task_provider: TaskProvider,
    /// Self-hosted GitLab host (exported as `GITLAB_HOST`); empty for gitlab.com.
    pub host: String,
    pub branch_prefix: String,
    /// Free-text guidance appended to workspace prompts.
    pub custom_instructions: String,
    /// Repository directory names (or `owner/repo`) that belong to this workspace.
    pub repos: Vec<String>,
}

/// The whole workspace configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Has the user completed first-run setup? Drives the setup wizard.
    pub initialized: bool,
    /// The development root that holds workspaces/worktrees (crow's `devRoot`).
    pub dev_root: Option<PathBuf>,
    pub default_provider: CodeProvider,
    pub default_task_provider: TaskProvider,
    /// Prepended to generated branch names (e.g. `feature/`).
    pub branch_prefix: String,
    /// Self-hosted GitLab host for the default workspace (exported as `GITLAB_HOST`).
    pub gitlab_host: String,
    pub jira: JiraConfig,
    pub automation: Automation,
    /// Repos hidden from the review board (supports `*` wildcards).
    pub exclude_review_repos: Vec<String>,
    /// Repos hidden from the ticket board / auto-create (supports `*` wildcards).
    pub exclude_ticket_repos: Vec<String>,
    /// How often (seconds) to poll GitHub/GitLab in the background. `0` = off.
    pub poll_seconds: u64,
    pub workspaces: Vec<WorkspaceConfig>,

    /// Where this loads from / saves to. Not serialized.
    #[serde(skip)]
    pub path: Option<PathBuf>,
    /// Editable text buffers mirroring the typed fields above (for the UI's
    /// `text_input`s, which must borrow a stable string). Kept in sync on edit.
    #[serde(skip)]
    pub poll_buf: String,
    #[serde(skip)]
    pub dev_root_buf: String,
    #[serde(skip)]
    pub exclude_review_buf: String,
    #[serde(skip)]
    pub exclude_ticket_buf: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            initialized: false,
            dev_root: None,
            default_provider: CodeProvider::default(),
            default_task_provider: TaskProvider::default(),
            branch_prefix: String::new(),
            gitlab_host: String::new(),
            jira: JiraConfig::default(),
            automation: Automation::default(),
            exclude_review_repos: Vec::new(),
            exclude_ticket_repos: Vec::new(),
            poll_seconds: 60,
            workspaces: Vec::new(),
            path: None,
            poll_buf: String::new(),
            dev_root_buf: String::new(),
            exclude_review_buf: String::new(),
            exclude_ticket_buf: String::new(),
        }
    }
}

impl Config {
    /// The default config file location in the OS data directory.
    pub fn default_path() -> Option<PathBuf> {
        directories::ProjectDirs::from("dev", "filament", "filament")
            .map(|d| d.data_local_dir().join("config.json"))
    }

    /// Load from the default location (defaults if absent/unreadable).
    pub fn load() -> Config {
        match Self::default_path() {
            Some(p) => Self::load_at(p),
            None => Config::default(),
        }
    }

    /// Load from an explicit path (defaults if absent/unreadable).
    pub fn load_at(path: PathBuf) -> Config {
        let mut cfg = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<Config>(&s).ok())
            .unwrap_or_default();
        cfg.path = Some(path);
        cfg.sync_buffers();
        cfg
    }

    /// Refresh the editable UI buffers from the typed fields.
    pub fn sync_buffers(&mut self) {
        self.poll_buf = self.poll_seconds.to_string();
        self.dev_root_buf = self
            .dev_root
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        self.exclude_review_buf = self.exclude_review_repos.join(", ");
        self.exclude_ticket_buf = self.exclude_ticket_repos.join(", ");
    }

    /// Persist the config, creating the parent directory as needed.
    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let data = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, data)
    }

    /// The workspace config (if any) that owns `repo` by directory name.
    pub fn workspace_for(&self, repo: &std::path::Path) -> Option<&WorkspaceConfig> {
        let name = repo_dir_name(repo);
        self.workspaces.iter().find(|w| {
            w.repos
                .iter()
                .any(|r| r.eq_ignore_ascii_case(&name) || glob_match(r, &name))
        })
    }

    /// The code provider to use for `repo` (workspace override, else default).
    pub fn provider_for(&self, repo: &std::path::Path) -> CodeProvider {
        self.workspace_for(repo)
            .map(|w| w.provider)
            .unwrap_or(self.default_provider)
    }

    /// The task provider to use for `repo` (workspace override, else default).
    pub fn task_provider_for(&self, repo: &std::path::Path) -> TaskProvider {
        self.workspace_for(repo)
            .map(|w| w.task_provider)
            .unwrap_or(self.default_task_provider)
    }

    /// The branch prefix to use for `repo` (workspace override, else default).
    pub fn branch_prefix_for(&self, repo: &std::path::Path) -> String {
        self.workspace_for(repo)
            .map(|w| w.branch_prefix.clone())
            .filter(|p| !p.is_empty())
            .unwrap_or_else(|| self.branch_prefix.clone())
    }

    /// The GitLab host to use for `repo` (workspace override, else default).
    pub fn host_for(&self, repo: &std::path::Path) -> Option<String> {
        let host = self
            .workspace_for(repo)
            .map(|w| w.host.clone())
            .filter(|h| !h.is_empty())
            .unwrap_or_else(|| self.gitlab_host.clone());
        (!host.is_empty()).then_some(host)
    }

    /// Whether `repo` is excluded from the review board.
    pub fn review_excluded(&self, repo: &str) -> bool {
        any_glob(&self.exclude_review_repos, repo)
    }

    /// Whether `repo` is excluded from the ticket board / auto-create.
    pub fn ticket_excluded(&self, repo: &str) -> bool {
        any_glob(&self.exclude_ticket_repos, repo)
    }
}

/// A repository's directory name (lowercased), for workspace matching.
fn repo_dir_name(repo: &std::path::Path) -> String {
    repo.file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}

fn any_glob(patterns: &[String], value: &str) -> bool {
    patterns.iter().any(|p| glob_match(p.trim(), value))
}

/// A tiny glob matcher supporting `*` (any run of chars, including empty),
/// matched case-insensitively. Used for repo exclude patterns like
/// `zarf-dev/*` and exact `owner/repo`.
pub fn glob_match(pattern: &str, value: &str) -> bool {
    if pattern.is_empty() {
        return false;
    }
    let pat: Vec<char> = pattern.to_ascii_lowercase().chars().collect();
    let val: Vec<char> = value.to_ascii_lowercase().chars().collect();
    // Classic two-pointer wildcard match with backtracking on `*`.
    let (mut p, mut v) = (0usize, 0usize);
    let (mut star, mut mark) = (None, 0usize);
    while v < val.len() {
        if p < pat.len() && (pat[p] == val[v] || pat[p] == '?') {
            p += 1;
            v += 1;
        } else if p < pat.len() && pat[p] == '*' {
            star = Some(p);
            mark = v;
            p += 1;
        } else if let Some(sp) = star {
            p = sp + 1;
            mark += 1;
            v = mark;
        } else {
            return false;
        }
    }
    while p < pat.len() && pat[p] == '*' {
        p += 1;
    }
    p == pat.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_matches_wildcards() {
        assert!(glob_match("zarf-dev/*", "zarf-dev/uds-core"));
        assert!(glob_match("bmlt-enabled/yap", "bmlt-enabled/yap"));
        assert!(!glob_match("zarf-dev/*", "other/repo"));
        assert!(glob_match("*", "anything/at-all"));
        assert!(glob_match("*-worktrees", "myrepo-worktrees"));
        assert!(!glob_match("foo", "foobar"));
        assert!(glob_match("FOO/*", "foo/bar")); // case-insensitive
    }

    #[test]
    fn defaults_are_crow_like() {
        let c = Config::default();
        assert!(!c.initialized);
        assert_eq!(c.poll_seconds, 60);
        assert_eq!(c.automation.auto_label, "crow:auto");
        assert_eq!(c.automation.merge_label, "crow:merge");
        assert!(!c.automation.auto_create);
        assert!(!c.automation.auto_merge);
        assert!(c.automation.auto_complete);
        assert!(c.automation.manager_auto_permission);
    }

    #[test]
    fn roundtrips_through_json() {
        let c = Config {
            initialized: true,
            branch_prefix: "feature/".into(),
            default_provider: CodeProvider::GitLab,
            exclude_ticket_repos: vec!["acme/*".into()],
            workspaces: vec![WorkspaceConfig {
                name: "Acme".into(),
                provider: CodeProvider::GitLab,
                task_provider: TaskProvider::Jira,
                branch_prefix: "wip/".into(),
                repos: vec!["acme-api".into()],
                ..Default::default()
            }],
            ..Default::default()
        };
        let json = serde_json::to_string(&c).unwrap();
        let back: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(back.branch_prefix, "feature/");
        assert_eq!(back.default_provider, CodeProvider::GitLab);
        assert!(back.ticket_excluded("acme/widget"));
        assert_eq!(
            back.branch_prefix_for(std::path::Path::new("/x/acme-api")),
            "wip/"
        );
        assert_eq!(
            back.provider_for(std::path::Path::new("/x/acme-api")),
            CodeProvider::GitLab
        );
    }

    #[test]
    fn jira_status_mapping() {
        let mut j = JiraConfig::default();
        j.status_map.insert("Ready".into(), "To Do".into());
        assert_eq!(j.status_for("Ready"), "To Do");
        assert_eq!(j.status_for("In Review"), "In Review");
    }
}
