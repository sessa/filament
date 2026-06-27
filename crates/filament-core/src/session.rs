//! Worktree-backed work **sessions** — a crow-style task workflow.
//!
//! Inspired by [`radiusmethod/crow`](https://github.com/radiusmethod/crow): a
//! *session* pairs a git **worktree** (an isolated checkout on its own branch)
//! with a Claude Code instance and, optionally, a linked GitHub **issue** and
//! **pull request**. Filament can create a session for an issue or a free-text
//! task, launch `claude` in its worktree, track the PR/CI status, and move the
//! session through `Working → Review → Done` as the PR opens and merges.
//!
//! This module owns the *model* and persistence; the git plumbing lives in
//! [`crate::git`] and the GitHub bits in [`crate::github`]. Everything here is
//! UI-free and unit-testable.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::backend::{CodeProvider, TaskProvider};
use crate::git;

/// Where a session sits in its lifecycle. The first three variants are the
/// board *pipeline* (their order is column order); `Paused` and `Archived` are
/// manual side states matching crow's session statuses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SessionState {
    /// Active development; no open PR yet.
    #[default]
    Working,
    /// A pull request is open and awaiting review/merge.
    Review,
    /// The PR merged or the issue closed.
    Done,
    /// Manually paused.
    Paused,
    /// Manually archived (kept for reference, hidden from the active board).
    Archived,
}

impl SessionState {
    pub fn label(self) -> &'static str {
        match self {
            SessionState::Working => "Working",
            SessionState::Review => "In Review",
            SessionState::Done => "Done",
            SessionState::Paused => "Paused",
            SessionState::Archived => "Archived",
        }
    }

    /// The lifecycle pipeline shown as board columns, in order.
    pub const BOARD: [SessionState; 3] = [
        SessionState::Working,
        SessionState::Review,
        SessionState::Done,
    ];

    /// Every state, for iteration.
    pub const ALL: [SessionState; 5] = [
        SessionState::Working,
        SessionState::Review,
        SessionState::Done,
        SessionState::Paused,
        SessionState::Archived,
    ];

    /// States a user can manually pin a session to (the rest are derived).
    pub const MANUAL: [SessionState; 4] = [
        SessionState::Working,
        SessionState::Review,
        SessionState::Paused,
        SessionState::Archived,
    ];

    pub fn parse(s: &str) -> Option<SessionState> {
        match s.trim().to_ascii_lowercase().as_str() {
            "working" | "active" => Some(SessionState::Working),
            "review" | "inreview" | "in-review" => Some(SessionState::Review),
            "done" | "completed" | "complete" => Some(SessionState::Done),
            "paused" | "pause" => Some(SessionState::Paused),
            "archived" | "archive" => Some(SessionState::Archived),
            _ => None,
        }
    }
}

/// A project-board column (crow's pipeline: Backlog → Ready → In Progress →
/// In Review → Done). Free-text statuses that don't map fall into [`ProjectStatus::Other`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectStatus {
    Backlog,
    Ready,
    InProgress,
    InReview,
    Done,
    Other(String),
}

impl ProjectStatus {
    /// The five standard pipeline columns, in order.
    pub const PIPELINE: [ProjectStatus; 5] = [
        ProjectStatus::Backlog,
        ProjectStatus::Ready,
        ProjectStatus::InProgress,
        ProjectStatus::InReview,
        ProjectStatus::Done,
    ];

    /// Bucket a raw project-board status name into a pipeline column.
    pub fn from_raw(raw: &str) -> ProjectStatus {
        let norm: String = raw
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .map(|c| c.to_ascii_lowercase())
            .collect();
        match norm.as_str() {
            "backlog" | "todo" | "new" | "open" => ProjectStatus::Backlog,
            "ready" | "selected" | "uptonext" | "next" => ProjectStatus::Ready,
            "inprogress" | "indevelopment" | "doing" | "started" => ProjectStatus::InProgress,
            "inreview" | "codereview" | "review" | "reviewing" => ProjectStatus::InReview,
            "done" | "closed" | "merged" | "shipped" | "complete" | "completed" => {
                ProjectStatus::Done
            }
            _ if raw.trim().is_empty() => ProjectStatus::Backlog,
            _ => ProjectStatus::Other(raw.trim().to_string()),
        }
    }

    pub fn label(&self) -> &str {
        match self {
            ProjectStatus::Backlog => "Backlog",
            ProjectStatus::Ready => "Ready",
            ProjectStatus::InProgress => "In Progress",
            ProjectStatus::InReview => "In Review",
            ProjectStatus::Done => "Done",
            ProjectStatus::Other(s) => s,
        }
    }
}

/// A linked issue / ticket (GitHub, GitLab, or Jira).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct IssueRef {
    pub number: u64,
    pub title: String,
    pub url: String,
    /// `OPEN` / `CLOSED` (as reported by the provider).
    #[serde(default)]
    pub state: String,
    /// Label names on the issue (for label-driven automation like `crow:auto`).
    #[serde(default)]
    pub labels: Vec<String>,
    /// The issue's project-board status name, when known (drives the ticket board).
    #[serde(default)]
    pub project_status: Option<String>,
    /// `owner/repo` (or Jira key prefix) the issue belongs to, for cross-repo boards.
    #[serde(default)]
    pub repo: Option<String>,
}

impl IssueRef {
    pub fn is_closed(&self) -> bool {
        self.state.eq_ignore_ascii_case("closed")
    }

    /// Whether the issue carries `label` (case-insensitive).
    pub fn has_label(&self, label: &str) -> bool {
        self.labels.iter().any(|l| l.eq_ignore_ascii_case(label))
    }

    /// The issue's project-board column, if a status is known.
    pub fn status(&self) -> Option<ProjectStatus> {
        self.project_status.as_deref().map(ProjectStatus::from_raw)
    }
}

/// Roll-up of a PR's CI checks.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckSummary {
    pub passing: u32,
    pub failing: u32,
    pub pending: u32,
}

impl CheckSummary {
    pub fn total(&self) -> u32 {
        self.passing + self.failing + self.pending
    }

    /// A single rolled-up verdict for badges.
    pub fn overall(&self) -> CheckState {
        if self.failing > 0 {
            CheckState::Failing
        } else if self.pending > 0 {
            CheckState::Pending
        } else if self.passing > 0 {
            CheckState::Passing
        } else {
            CheckState::None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckState {
    Passing,
    Failing,
    Pending,
    None,
}

/// A linked pull request (GitHub) or merge request (GitLab).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PrRef {
    pub number: u64,
    #[serde(default)]
    pub title: String,
    pub url: String,
    /// `OPEN` / `MERGED` / `CLOSED`.
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub is_draft: bool,
    /// `APPROVED` / `CHANGES_REQUESTED` / `REVIEW_REQUIRED`, when known.
    #[serde(default)]
    pub review_decision: Option<String>,
    #[serde(default)]
    pub checks: CheckSummary,
    /// `MERGEABLE` / `CONFLICTING` / `UNKNOWN`, when known.
    #[serde(default)]
    pub mergeable: Option<String>,
    /// `CLEAN` / `BLOCKED` / `DIRTY` / `BEHIND` / `UNSTABLE` …, when known.
    #[serde(default)]
    pub merge_state_status: Option<String>,
    /// Label names on the PR (for label-driven automation like `crow:merge`).
    #[serde(default)]
    pub labels: Vec<String>,
    /// The PR's head (source) branch, when known — needed to start a review worktree.
    #[serde(default)]
    pub head: Option<String>,
}

impl PrRef {
    pub fn is_merged(&self) -> bool {
        self.state.eq_ignore_ascii_case("merged")
    }
    pub fn is_open(&self) -> bool {
        self.state.eq_ignore_ascii_case("open")
    }
    pub fn is_closed(&self) -> bool {
        self.state.eq_ignore_ascii_case("closed")
    }
    /// Whether git reports the branch as conflicting with its base.
    pub fn is_conflicting(&self) -> bool {
        self.mergeable
            .as_deref()
            .is_some_and(|m| m.eq_ignore_ascii_case("conflicting"))
    }
    /// Whether the PR is cleanly mergeable (no conflicts reported).
    pub fn is_mergeable(&self) -> bool {
        self.mergeable
            .as_deref()
            .is_some_and(|m| m.eq_ignore_ascii_case("mergeable"))
    }
    pub fn is_approved(&self) -> bool {
        self.review_decision
            .as_deref()
            .is_some_and(|d| d.eq_ignore_ascii_case("approved"))
    }
    pub fn changes_requested(&self) -> bool {
        self.review_decision
            .as_deref()
            .is_some_and(|d| d.eq_ignore_ascii_case("changes_requested"))
    }
    /// Whether the PR carries `label` (case-insensitive).
    pub fn has_label(&self, label: &str) -> bool {
        self.labels.iter().any(|l| l.eq_ignore_ascii_case(label))
    }
    /// A merge-readiness verdict for badges.
    pub fn merge_readiness(&self) -> MergeReadiness {
        if self.is_merged() {
            MergeReadiness::Merged
        } else if self.is_conflicting() {
            MergeReadiness::Conflicting
        } else if self.is_mergeable() {
            MergeReadiness::Mergeable
        } else {
            MergeReadiness::Unknown
        }
    }
}

/// A single rolled-up merge-readiness verdict for badges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeReadiness {
    Mergeable,
    Conflicting,
    Merged,
    Unknown,
}

/// An arbitrary reference link attached to a session (crow's `add-link`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SessionLink {
    pub label: String,
    pub url: String,
    /// A free-text kind ("design", "doc", "pr", …); defaults to "link".
    #[serde(default)]
    pub kind: String,
}

/// A terminal tab belonging to a session (crow's per-session managed terminals).
/// The live terminal lives in the UI; this is the persisted metadata so the CLI
/// can list / rename / close terminals and they survive a restart.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct TerminalRec {
    /// Stable id used by the CLI (`--terminal <id>`).
    pub id: String,
    pub name: String,
    pub cwd: PathBuf,
    /// "claude" / "shell" / "manager".
    #[serde(default)]
    pub kind: String,
    /// An explicit command to run instead of the default for `kind`.
    #[serde(default)]
    pub command: Option<String>,
}

/// A single work session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Session {
    /// Stable, unique slug used as the key and worktree directory name.
    pub id: String,
    pub title: String,
    pub repo_root: PathBuf,
    pub worktree: PathBuf,
    pub branch: String,
    pub base_branch: String,
    #[serde(default)]
    pub issue: Option<IssueRef>,
    #[serde(default)]
    pub pr: Option<PrRef>,
    pub state: SessionState,
    /// A user-pinned state that overrides derivation (e.g. Paused/Archived/Review)
    /// until the session auto-completes. `None` means "follow the PR/issue".
    #[serde(default)]
    pub manual_state: Option<SessionState>,
    /// Unix seconds; `0` when unknown.
    #[serde(default)]
    pub created_unix: u64,
    /// When the GitHub/GitLab data was last synced (Unix seconds; `0` if never).
    #[serde(default)]
    pub last_synced_unix: u64,
    /// Reference links attached to the session.
    #[serde(default)]
    pub links: Vec<SessionLink>,
    /// Which forge drives this session's PR/CI.
    #[serde(default)]
    pub provider: CodeProvider,
    /// Which tracker drives this session's issue/ticket.
    #[serde(default)]
    pub task_provider: TaskProvider,
    /// Set once automation has suggested opening a PR, so it doesn't nag again.
    #[serde(default)]
    pub pr_suggested: bool,
    /// Persisted terminal tabs for this session (the live terminals live in the UI).
    #[serde(default)]
    pub terminals: Vec<TerminalRec>,
}

impl Session {
    /// The lifecycle state derived purely from the linked PR/issue (no manual
    /// override applied). This is the board pipeline position. Completion on a
    /// *closed issue* requires [`Self::has_work_evidence`] so a session attached
    /// to an already-closed ticket isn't instantly marked done (matching crow).
    pub fn derive_state(&self) -> SessionState {
        if let Some(pr) = &self.pr {
            if pr.is_merged() {
                return SessionState::Done;
            }
            if pr.is_open() {
                return SessionState::Review;
            }
        }
        if let Some(issue) = &self.issue {
            if issue.is_closed() && self.has_work_evidence() {
                return SessionState::Done;
            }
        }
        SessionState::Working
    }

    /// Whether the linked PR merged or the linked issue closed.
    pub fn auto_completed(&self) -> bool {
        self.pr.as_ref().is_some_and(|p| p.is_merged())
            || self.issue.as_ref().is_some_and(|i| i.is_closed())
    }

    /// "Positive evidence" that real work happened in this session — used to
    /// guard auto-completion so a freshly-created session attached to an already
    /// closed issue isn't instantly marked done (matching crow).
    pub fn has_work_evidence(&self) -> bool {
        self.pr.is_some() || (self.branch != self.base_branch && !self.branch.is_empty())
    }

    /// The state to display: completion (with evidence) wins, then any manual
    /// pin, then the derived pipeline position.
    pub fn effective_state(&self) -> SessionState {
        if let Some(m) = self.manual_state {
            // A completing session overrides a manual pause/archive/review.
            if self.auto_completed() && self.has_work_evidence() {
                return SessionState::Done;
            }
            return m;
        }
        self.derive_state()
    }

    /// Recompute and store [`Self::effective_state`].
    pub fn sync_state(&mut self) {
        self.state = self.effective_state();
    }

    /// Pin (or, with `None`, unpin) a manual state and recompute.
    pub fn set_manual(&mut self, state: Option<SessionState>) {
        self.manual_state = state;
        self.sync_state();
    }

    /// Whether the worktree directory still exists on disk.
    pub fn worktree_exists(&self) -> bool {
        self.worktree.is_dir()
    }
}

/// A request to create a new session.
#[derive(Debug, Clone, Default)]
pub struct NewSession {
    pub title: String,
    pub base_branch: String,
    pub issue: Option<IssueRef>,
    pub provider: CodeProvider,
    pub task_provider: TaskProvider,
}

/// The on-disk store of all sessions across repos.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SessionStore {
    /// The file this store loads from / saves to. Not serialized.
    #[serde(skip)]
    pub path: PathBuf,
    #[serde(default)]
    pub sessions: Vec<Session>,
}

impl SessionStore {
    /// The default store location in the OS data directory.
    pub fn default_path() -> Option<PathBuf> {
        directories::ProjectDirs::from("dev", "filament", "filament")
            .map(|d| d.data_local_dir().join("sessions.json"))
    }

    /// Load from the default location (empty store if absent/unreadable).
    pub fn load() -> SessionStore {
        match Self::default_path() {
            Some(p) => Self::load_at(p),
            None => SessionStore::default(),
        }
    }

    /// Load from an explicit path (empty store if absent/unreadable).
    pub fn load_at(path: PathBuf) -> SessionStore {
        let sessions = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<SessionStore>(&s).ok())
            .map(|s| s.sessions)
            .unwrap_or_default();
        SessionStore { path, sessions }
    }

    /// Persist the store, creating the parent directory as needed.
    pub fn save(&self) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let data = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(&self.path, data)
    }

    /// Sessions belonging to `repo`, newest first.
    pub fn for_repo<'a>(&'a self, repo: &'a Path) -> impl Iterator<Item = &'a Session> + 'a {
        let repo = canonical(repo);
        self.sessions
            .iter()
            .filter(move |s| canonical(&s.repo_root) == repo)
    }

    pub fn get(&self, id: &str) -> Option<&Session> {
        self.sessions.iter().find(|s| s.id == id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Session> {
        self.sessions.iter_mut().find(|s| s.id == id)
    }

    /// Insert `session`, replacing any existing one with the same id.
    pub fn upsert(&mut self, session: Session) {
        match self.sessions.iter_mut().find(|s| s.id == session.id) {
            Some(existing) => *existing = session,
            None => self.sessions.push(session),
        }
    }

    /// Remove and return the session with `id`, if present.
    pub fn remove(&mut self, id: &str) -> Option<Session> {
        let idx = self.sessions.iter().position(|s| s.id == id)?;
        Some(self.sessions.remove(idx))
    }

    /// Whether some session already owns the worktree at `path`.
    pub fn owns_worktree(&self, path: &Path) -> bool {
        let path = canonical(path);
        self.sessions.iter().any(|s| canonical(&s.worktree) == path)
    }

    /// A unique id derived from `base` (`base`, `base-2`, `base-3`, …).
    pub fn unique_id(&self, base: &str) -> String {
        let base = if base.is_empty() { "session" } else { base };
        if self.get(base).is_none() {
            return base.to_string();
        }
        (2..)
            .map(|n| format!("{base}-{n}"))
            .find(|cand| self.get(cand).is_none())
            .unwrap_or_else(|| base.to_string())
    }
}

/// Canonicalize for path comparisons, falling back to the input when the path
/// doesn't exist yet.
fn canonical(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}

/// The directory under which a repo's worktrees are created:
/// a sibling `"<repo-name>-worktrees"` folder.
pub fn worktree_base(repo_root: &Path) -> PathBuf {
    let name = repo_root
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "repo".to_string());
    match repo_root.parent() {
        Some(parent) => parent.join(format!("{name}-worktrees")),
        None => repo_root.join(".filament-worktrees"),
    }
}

/// Turn arbitrary text into a filesystem/branch-safe slug (lowercase
/// alphanumerics joined by single dashes, capped at `max` chars).
pub fn slug(input: &str, max: usize) -> String {
    let mut s = String::new();
    let mut prev_dash = false;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            s.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !s.is_empty() && !prev_dash {
            s.push('-');
            prev_dash = true;
        }
    }
    while s.ends_with('-') {
        s.pop();
    }
    if s.chars().count() > max {
        s = s.chars().take(max).collect();
        while s.ends_with('-') {
            s.pop();
        }
    }
    if s.is_empty() {
        "session".to_string()
    } else {
        s
    }
}

/// A branch name that doesn't yet exist in `repo`, derived from `base`.
fn unique_branch(repo: &Path, base: &str) -> String {
    if !git::branch_exists(repo, base) {
        return base.to_string();
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|cand| !git::branch_exists(repo, cand))
        .unwrap_or_else(|| base.to_string())
}

/// The slug stem for a session, from its issue (`issue-123-title`) or title.
pub fn stem_for(req: &NewSession) -> String {
    match &req.issue {
        Some(issue) => format!("issue-{}-{}", issue.number, slug(&issue.title, 30)),
        None => slug(&req.title, 40),
    }
}

/// Create a worktree + branch and register the session in `store`.
///
/// `branch_prefix` (e.g. `"feature/"`) is prepended to the generated branch
/// name, matching crow's per-workspace `branchPrefix`. The caller is responsible
/// for persisting `store` afterwards. `now_unix` is injected (rather than read
/// here) so this stays pure and testable.
pub fn create_session(
    store: &mut SessionStore,
    repo_root: &Path,
    req: NewSession,
    branch_prefix: &str,
    now_unix: u64,
) -> Result<Session, git::GitError> {
    let stem = stem_for(&req);
    let branch = unique_branch(repo_root, &format!("{}{stem}", branch_prefix.trim()));
    let id = store.unique_id(&slug(&stem, 48));
    let worktree = worktree_base(repo_root).join(&id);

    git::add_worktree(repo_root, &worktree, &branch, &req.base_branch)?;

    let session = Session {
        id,
        title: if req.title.trim().is_empty() {
            branch.clone()
        } else {
            req.title.trim().to_string()
        },
        repo_root: repo_root.to_path_buf(),
        worktree,
        branch,
        base_branch: req.base_branch,
        issue: req.issue,
        pr: None,
        state: SessionState::Working,
        manual_state: None,
        created_unix: now_unix,
        last_synced_unix: 0,
        links: Vec::new(),
        provider: req.provider,
        task_provider: req.task_provider,
        pr_suggested: false,
        terminals: Vec::new(),
    };
    store.upsert(session.clone());
    Ok(session)
}

/// Create a session for reviewing an existing PR: a worktree on the PR's
/// **existing** head `branch` (not a fresh branch), pinned to the Review state.
pub fn create_review_session(
    store: &mut SessionStore,
    repo_root: &Path,
    branch: &str,
    pr: PrRef,
    provider: CodeProvider,
    now_unix: u64,
) -> Result<Session, git::GitError> {
    let stem = slug(&format!("review-{}", pr.number), 48);
    let id = store.unique_id(&stem);
    let worktree = worktree_base(repo_root).join(&id);
    git::add_worktree_existing(repo_root, &worktree, branch)?;

    let mut session = Session {
        id,
        title: format!("Review #{}: {}", pr.number, pr.title),
        repo_root: repo_root.to_path_buf(),
        worktree,
        branch: branch.to_string(),
        base_branch: git::default_branch(repo_root).unwrap_or_else(|| "main".to_string()),
        pr: Some(pr),
        provider,
        created_unix: now_unix,
        ..Session::default()
    };
    session.manual_state = Some(SessionState::Review);
    session.sync_state();
    store.upsert(session.clone());
    Ok(session)
}

/// Remove a session, optionally deleting its git worktree.
///
/// This is "safe deletion" in two senses, both matching crow:
/// - a worktree on a *protected* branch (the base/default branch) is always kept
///   on disk; and
/// - the worktree is removed **without** `--force`, so git refuses (and this
///   returns an error, leaving the session tracked) when it has uncommitted or
///   untracked changes — never silently discarding work.
pub fn remove_session(
    store: &mut SessionStore,
    id: &str,
    delete_worktree: bool,
) -> Result<(), git::GitError> {
    let Some(session) = store.get(id).cloned() else {
        return Ok(());
    };
    let protected = session.branch == session.base_branch
        || git::default_branch(&session.repo_root).as_deref() == Some(session.branch.as_str());
    if delete_worktree && !protected && session.worktree.exists() {
        git::remove_worktree(&session.repo_root, &session.worktree, false)?;
    }
    store.remove(id);
    Ok(())
}

/// Worktrees of `repo_root` that aren't tracked by any session (and aren't the
/// main worktree or a bare repo) — candidates for adoption.
pub fn detect_orphans(store: &SessionStore, repo_root: &Path) -> Vec<git::Worktree> {
    let root = canonical(repo_root);
    git::list_worktrees(repo_root)
        .into_iter()
        .filter(|w| !w.bare && canonical(&w.path) != root && !store.owns_worktree(&w.path))
        .collect()
}

/// Adopt an orphaned worktree as a `Working` session.
pub fn adopt_orphan(
    store: &mut SessionStore,
    repo_root: &Path,
    worktree: &git::Worktree,
    now_unix: u64,
) -> Session {
    let branch = worktree
        .branch
        .clone()
        .unwrap_or_else(|| "detached".to_string());
    let name = worktree
        .path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| branch.clone());
    let id = store.unique_id(&slug(&name, 48));
    let session = Session {
        id,
        title: branch.clone(),
        repo_root: repo_root.to_path_buf(),
        worktree: worktree.path.clone(),
        branch,
        base_branch: git::default_branch(repo_root).unwrap_or_else(|| "main".to_string()),
        created_unix: now_unix,
        ..Session::default()
    };
    store.upsert(session.clone());
    session
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_is_safe_and_capped() {
        assert_eq!(slug("Add OAuth to the API!", 40), "add-oauth-to-the-api");
        assert_eq!(slug("  --weird-- name  ", 40), "weird-name");
        assert_eq!(slug("", 40), "session");
        assert_eq!(slug("a very very very long title here", 10), "a-very-ver");
    }

    #[test]
    fn unique_id_disambiguates() {
        let mut store = SessionStore::default();
        assert_eq!(store.unique_id("foo"), "foo");
        store.sessions.push(Session {
            id: "foo".into(),
            ..Session::default()
        });
        assert_eq!(store.unique_id("foo"), "foo-2");
    }

    #[test]
    fn worktree_base_is_a_sibling() {
        let base = worktree_base(Path::new("/code/myrepo"));
        assert_eq!(base, PathBuf::from("/code/myrepo-worktrees"));
    }

    #[test]
    fn derive_state_follows_pr_and_issue() {
        let mut s = Session {
            id: "x".into(),
            branch: "b".into(),
            base_branch: "main".into(),
            ..Session::default()
        };
        assert_eq!(s.derive_state(), SessionState::Working);
        s.issue = Some(IssueRef {
            number: 1,
            title: "t".into(),
            url: "u".into(),
            state: "CLOSED".into(),
            ..IssueRef::default()
        });
        assert_eq!(s.derive_state(), SessionState::Done);
        s.pr = Some(PrRef {
            number: 2,
            title: "p".into(),
            url: "u".into(),
            state: "OPEN".into(),
            ..PrRef::default()
        });
        assert_eq!(s.derive_state(), SessionState::Review);
        s.pr.as_mut().unwrap().state = "MERGED".into();
        assert_eq!(s.derive_state(), SessionState::Done);
    }

    #[test]
    fn effective_state_honors_manual_and_completion() {
        let mut s = Session {
            id: "x".into(),
            branch: "feature".into(),
            base_branch: "main".into(),
            ..Session::default()
        };
        // Manual pin wins over derivation.
        s.set_manual(Some(SessionState::Paused));
        assert_eq!(s.state, SessionState::Paused);
        // A merged PR (with work evidence) completes regardless of the pin.
        s.pr = Some(PrRef {
            number: 1,
            state: "MERGED".into(),
            ..PrRef::default()
        });
        assert_eq!(s.effective_state(), SessionState::Done);
        s.sync_state();
        assert_eq!(s.state, SessionState::Done);
    }

    #[test]
    fn auto_complete_needs_work_evidence() {
        // A brand-new session attached to an already-closed issue, with no PR and
        // no branch divergence, must NOT auto-complete.
        let s = Session {
            id: "x".into(),
            branch: "main".into(),
            base_branch: "main".into(),
            issue: Some(IssueRef {
                number: 1,
                state: "CLOSED".into(),
                ..IssueRef::default()
            }),
            ..Session::default()
        };
        assert!(s.auto_completed());
        assert!(!s.has_work_evidence());
        assert_ne!(s.effective_state(), SessionState::Done);
    }

    #[test]
    fn project_status_buckets() {
        assert_eq!(
            ProjectStatus::from_raw("In Progress"),
            ProjectStatus::InProgress
        );
        assert_eq!(
            ProjectStatus::from_raw("Code Review"),
            ProjectStatus::InReview
        );
        assert_eq!(ProjectStatus::from_raw("To Do"), ProjectStatus::Backlog);
        assert_eq!(
            ProjectStatus::from_raw("Blocked"),
            ProjectStatus::Other("Blocked".into())
        );
    }

    #[test]
    fn pr_merge_readiness() {
        let pr = PrRef {
            mergeable: Some("CONFLICTING".into()),
            ..PrRef::default()
        };
        assert!(pr.is_conflicting());
        assert_eq!(pr.merge_readiness(), MergeReadiness::Conflicting);
    }

    #[test]
    fn check_summary_overall() {
        assert_eq!(CheckSummary::default().overall(), CheckState::None);
        assert_eq!(
            CheckSummary {
                passing: 3,
                failing: 0,
                pending: 0
            }
            .overall(),
            CheckState::Passing
        );
        assert_eq!(
            CheckSummary {
                passing: 3,
                failing: 0,
                pending: 1
            }
            .overall(),
            CheckState::Pending
        );
        assert_eq!(
            CheckSummary {
                passing: 3,
                failing: 1,
                pending: 1
            }
            .overall(),
            CheckState::Failing
        );
    }
}
