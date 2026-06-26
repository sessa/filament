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

use crate::git;

/// Where a session sits in its lifecycle. Variant order is also column order in
/// the board UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionState {
    /// Active development; no open PR yet.
    Working,
    /// A pull request is open and awaiting review/merge.
    Review,
    /// The PR merged or the issue closed.
    Done,
}

impl SessionState {
    pub fn label(self) -> &'static str {
        match self {
            SessionState::Working => "Working",
            SessionState::Review => "In Review",
            SessionState::Done => "Done",
        }
    }

    pub const ALL: [SessionState; 3] = [
        SessionState::Working,
        SessionState::Review,
        SessionState::Done,
    ];
}

/// A linked GitHub issue.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IssueRef {
    pub number: u64,
    pub title: String,
    pub url: String,
    /// `OPEN` / `CLOSED` (as reported by `gh`).
    #[serde(default)]
    pub state: String,
}

impl IssueRef {
    pub fn is_closed(&self) -> bool {
        self.state.eq_ignore_ascii_case("closed")
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

/// A linked GitHub pull request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
}

impl PrRef {
    pub fn is_merged(&self) -> bool {
        self.state.eq_ignore_ascii_case("merged")
    }
    pub fn is_open(&self) -> bool {
        self.state.eq_ignore_ascii_case("open")
    }
}

/// A single work session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
    /// Unix seconds; `0` when unknown.
    #[serde(default)]
    pub created_unix: u64,
}

impl Session {
    /// Recompute the lifecycle state from the linked PR/issue.
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
            if issue.is_closed() {
                return SessionState::Done;
            }
        }
        SessionState::Working
    }

    /// Whether the worktree directory still exists on disk.
    pub fn worktree_exists(&self) -> bool {
        self.worktree.is_dir()
    }
}

/// A request to create a new session.
#[derive(Debug, Clone)]
pub struct NewSession {
    pub title: String,
    pub base_branch: String,
    pub issue: Option<IssueRef>,
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
/// The caller is responsible for persisting `store` afterwards. `now_unix` is
/// injected (rather than read here) so this stays pure and testable.
pub fn create_session(
    store: &mut SessionStore,
    repo_root: &Path,
    req: NewSession,
    now_unix: u64,
) -> Result<Session, git::GitError> {
    let stem = stem_for(&req);
    let branch = unique_branch(repo_root, &stem);
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
        created_unix: now_unix,
    };
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
        issue: None,
        pr: None,
        state: SessionState::Working,
        created_unix: now_unix,
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
            title: "foo".into(),
            repo_root: "/r".into(),
            worktree: "/w".into(),
            branch: "foo".into(),
            base_branch: "main".into(),
            issue: None,
            pr: None,
            state: SessionState::Working,
            created_unix: 0,
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
            title: "x".into(),
            repo_root: "/r".into(),
            worktree: "/w".into(),
            branch: "b".into(),
            base_branch: "main".into(),
            issue: None,
            pr: None,
            state: SessionState::Working,
            created_unix: 0,
        };
        assert_eq!(s.derive_state(), SessionState::Working);
        s.issue = Some(IssueRef {
            number: 1,
            title: "t".into(),
            url: "u".into(),
            state: "CLOSED".into(),
        });
        assert_eq!(s.derive_state(), SessionState::Done);
        s.pr = Some(PrRef {
            number: 2,
            title: "p".into(),
            url: "u".into(),
            state: "OPEN".into(),
            is_draft: false,
            review_decision: None,
            checks: CheckSummary::default(),
        });
        assert_eq!(s.derive_state(), SessionState::Review);
        s.pr.as_mut().unwrap().state = "MERGED".into();
        assert_eq!(s.derive_state(), SessionState::Done);
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
