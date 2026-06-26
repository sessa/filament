//! A thin, dependency-free wrapper over the `git` CLI.
//!
//! The session feature (crow-style worktree-per-task workflow) needs to create,
//! list, and remove **git worktrees** and introspect branches. Rather than pull
//! in a libgit2 binding, we shell out to the user's `git` — the same tool they
//! already use — and parse its porcelain output. Every call returns a
//! [`GitError`] on failure (including `git` not being installed) so the UI can
//! degrade gracefully instead of panicking.

use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("git is not installed or not on PATH")]
    NotInstalled,
    #[error("git {0} failed: {1}")]
    Failed(String, String),
}

/// Run `git -C <repo> <args...>` and return stdout on success.
pub fn run(repo: &Path, args: &[&str]) -> Result<String, GitError> {
    let output = Command::new("git").arg("-C").arg(repo).args(args).output();
    let output = match output {
        Ok(o) => o,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(GitError::NotInstalled),
        Err(e) => return Err(GitError::Failed(args.join(" "), e.to_string())),
    };
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(GitError::Failed(
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ))
    }
}

/// The top-level working directory of the repository containing `start`.
pub fn repo_root(start: &Path) -> Option<PathBuf> {
    let out = run(start, &["rev-parse", "--show-toplevel"]).ok()?;
    let line = out.lines().next()?.trim();
    (!line.is_empty()).then(|| PathBuf::from(line))
}

/// The currently checked-out branch of `repo`, or `None` when detached.
pub fn current_branch(repo: &Path) -> Option<String> {
    let b = run(repo, &["rev-parse", "--abbrev-ref", "HEAD"]).ok()?;
    let b = b.trim().to_string();
    (!b.is_empty() && b != "HEAD").then_some(b)
}

/// Whether a ref (branch name) already exists in `repo`.
pub fn branch_exists(repo: &Path, name: &str) -> bool {
    run(repo, &["rev-parse", "--verify", "--quiet", name])
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
}

/// All local branch names, sorted by most-recent commit first.
pub fn list_branches(repo: &Path) -> Vec<String> {
    run(
        repo,
        &[
            "branch",
            "--sort=-committerdate",
            "--format=%(refname:short)",
        ],
    )
    .map(|s| {
        s.lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect()
    })
    .unwrap_or_default()
}

/// The repository's default branch — `origin/HEAD` if known, else `main`/
/// `master` if present, else the current branch.
pub fn default_branch(repo: &Path) -> Option<String> {
    if let Ok(s) = run(
        repo,
        &[
            "symbolic-ref",
            "--quiet",
            "--short",
            "refs/remotes/origin/HEAD",
        ],
    ) {
        if let Some(name) = s.trim().rsplit('/').next() {
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }
    for cand in ["main", "master"] {
        if branch_exists(repo, cand) {
            return Some(cand.to_string());
        }
    }
    current_branch(repo)
}

/// One entry from `git worktree list --porcelain`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    pub path: PathBuf,
    pub head: Option<String>,
    /// Short branch name (the `refs/heads/` prefix is stripped).
    pub branch: Option<String>,
    pub bare: bool,
    pub detached: bool,
    pub locked: bool,
}

/// Parse the porcelain output of `git worktree list --porcelain`.
///
/// Records are separated by blank lines; each begins with a `worktree <path>`
/// line followed by zero or more attribute lines (`HEAD`, `branch`, `bare`,
/// `detached`, `locked`).
pub fn parse_worktrees(porcelain: &str) -> Vec<Worktree> {
    let mut out = Vec::new();
    let mut cur: Option<Worktree> = None;
    let push = |out: &mut Vec<Worktree>, cur: &mut Option<Worktree>| {
        if let Some(w) = cur.take() {
            out.push(w);
        }
    };
    for line in porcelain.lines() {
        if line.trim().is_empty() {
            push(&mut out, &mut cur);
            continue;
        }
        let (key, val) = line.split_once(' ').unwrap_or((line, ""));
        match key {
            "worktree" => {
                push(&mut out, &mut cur);
                cur = Some(Worktree {
                    path: PathBuf::from(val),
                    head: None,
                    branch: None,
                    bare: false,
                    detached: false,
                    locked: false,
                });
            }
            "HEAD" => {
                if let Some(w) = &mut cur {
                    w.head = Some(val.to_string());
                }
            }
            "branch" => {
                if let Some(w) = &mut cur {
                    w.branch = Some(val.trim_start_matches("refs/heads/").to_string());
                }
            }
            "bare" => {
                if let Some(w) = &mut cur {
                    w.bare = true;
                }
            }
            "detached" => {
                if let Some(w) = &mut cur {
                    w.detached = true;
                }
            }
            "locked" => {
                if let Some(w) = &mut cur {
                    w.locked = true;
                }
            }
            _ => {}
        }
    }
    push(&mut out, &mut cur);
    out
}

/// All worktrees registered for `repo` (the first entry is the main worktree).
pub fn list_worktrees(repo: &Path) -> Vec<Worktree> {
    run(repo, &["worktree", "list", "--porcelain"])
        .map(|s| parse_worktrees(&s))
        .unwrap_or_default()
}

/// Create a new worktree at `path` on a new branch `branch` based on `base`.
pub fn add_worktree(repo: &Path, path: &Path, branch: &str, base: &str) -> Result<(), GitError> {
    let p = path.to_string_lossy();
    run(repo, &["worktree", "add", "-b", branch, p.as_ref(), base])?;
    Ok(())
}

/// Remove the worktree at `path` (use `force` to discard uncommitted changes).
pub fn remove_worktree(repo: &Path, path: &Path, force: bool) -> Result<(), GitError> {
    let p = path.to_string_lossy();
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(p.as_ref());
    run(repo, &args)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_porcelain_worktrees() {
        let input = "\
worktree /home/me/proj
HEAD 1111111111111111111111111111111111111111
branch refs/heads/main

worktree /home/me/proj-worktrees/feature
HEAD 2222222222222222222222222222222222222222
branch refs/heads/feature/x

worktree /home/me/proj-worktrees/detached
HEAD 3333333333333333333333333333333333333333
detached

worktree /home/me/bare
bare
";
        let wts = parse_worktrees(input);
        assert_eq!(wts.len(), 4);
        assert_eq!(wts[0].path, PathBuf::from("/home/me/proj"));
        assert_eq!(wts[0].branch.as_deref(), Some("main"));
        assert_eq!(wts[1].branch.as_deref(), Some("feature/x"));
        assert!(wts[2].detached);
        assert_eq!(wts[2].branch, None);
        assert!(wts[3].bare);
    }
}
