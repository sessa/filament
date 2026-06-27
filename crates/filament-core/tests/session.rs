//! Integration tests for the worktree-backed session workflow.
//!
//! These drive the real `git` CLI against a throwaway repo in a tempdir, so they
//! exercise [`filament_core::git`] and [`filament_core::session`] end to end
//! (worktree creation, orphan detection, safe deletion) without any GitHub
//! dependency.

use std::path::Path;
use std::process::Command;

use filament_core::git;
use filament_core::session::{self, NewSession, Session, SessionState, SessionStore};

/// Initialize a git repo with one commit on `main` and return its path.
fn init_repo(dir: &Path) {
    let run = |args: &[&str]| {
        let status = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .status()
            .expect("git available");
        assert!(status.success(), "git {args:?} failed");
    };
    run(&["init", "-q", "-b", "main"]);
    run(&["config", "user.email", "test@example.com"]);
    run(&["config", "user.name", "Test"]);
    std::fs::write(dir.join("README.md"), "# test\n").unwrap();
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "initial"]);
}

#[test]
fn create_lists_and_removes_a_session() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    init_repo(&repo);

    let mut store = SessionStore {
        path: tmp.path().join("sessions.json"),
        sessions: Vec::new(),
    };

    let session = session::create_session(
        &mut store,
        &repo,
        NewSession {
            title: "Add OAuth to the API".into(),
            base_branch: "main".into(),
            issue: None,
            ..NewSession::default()
        },
        "",
        1_700_000_000,
    )
    .expect("create session");

    // Worktree exists on disk and is registered with git.
    assert!(session.worktree.is_dir(), "worktree dir created");
    assert_eq!(session.state, SessionState::Working);
    assert_eq!(session.branch, "add-oauth-to-the-api");
    let worktrees = git::list_worktrees(&repo);
    assert!(
        worktrees
            .iter()
            .any(|w| w.branch.as_deref() == Some("add-oauth-to-the-api")),
        "git knows about the new worktree: {worktrees:?}"
    );

    // Store persists and reloads.
    store.save().unwrap();
    let reloaded = SessionStore::load_at(store.path.clone());
    assert_eq!(reloaded.sessions.len(), 1);
    assert_eq!(reloaded.for_repo(&repo).count(), 1);

    // Safe removal deletes the (non-protected) worktree.
    let wt = session.worktree.clone();
    session::remove_session(&mut store, &session.id, true).expect("remove");
    assert!(!wt.exists(), "worktree removed from disk");
    assert_eq!(store.sessions.len(), 0);
}

#[test]
fn detects_and_adopts_orphan_worktrees() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    init_repo(&repo);

    // Create a worktree directly via git (an "orphan" — no session tracks it).
    let orphan_path = tmp.path().join("orphan-wt");
    git::add_worktree(&repo, &orphan_path, "stray-branch", "main").unwrap();

    let mut store = SessionStore {
        path: tmp.path().join("sessions.json"),
        sessions: Vec::new(),
    };

    let orphans = session::detect_orphans(&store, &repo);
    assert_eq!(orphans.len(), 1, "the main worktree is not an orphan");
    assert_eq!(orphans[0].branch.as_deref(), Some("stray-branch"));

    let adopted = session::adopt_orphan(&mut store, &repo, &orphans[0], 0);
    assert_eq!(adopted.branch, "stray-branch");
    // Once adopted it's no longer an orphan.
    assert_eq!(session::detect_orphans(&store, &repo).len(), 0);
}

#[test]
fn protected_branch_worktree_is_kept_on_removal() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    init_repo(&repo);

    // A session whose branch *is* its base branch is protected: removing it must
    // not delete the worktree. (Constructed directly — git won't let `main` be
    // checked out in two worktrees at once.)
    let wt = tmp.path().join("main-wt");
    std::fs::create_dir(&wt).unwrap();
    let mut store = SessionStore {
        path: tmp.path().join("sessions.json"),
        sessions: vec![Session {
            id: "protected".into(),
            title: "main".into(),
            repo_root: repo.clone(),
            worktree: wt.clone(),
            branch: "main".into(),
            base_branch: "main".into(),
            state: SessionState::Working,
            ..Session::default()
        }],
    };

    session::remove_session(&mut store, "protected", true).unwrap();
    assert!(
        wt.exists(),
        "protected-branch worktree is preserved on disk"
    );
    assert!(store.get("protected").is_none(), "session still untracked");
}
