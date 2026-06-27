//! GitHub integration via the `gh` CLI.
//!
//! Like crow, Filament shells out to the user's authenticated [`gh`](https://cli.github.com)
//! to read issues and pull-request / CI status and to drive PR automation. `gh`
//! is **optional**: when it's missing or unauthenticated every call returns a
//! typed [`GhError`] so the UI shows a quiet "GitHub CLI unavailable" hint
//! instead of failing. The JSON parsing is factored into pure functions that are
//! unit-tested against captured `gh ... --json` fixtures (so the parsing path is
//! covered without `gh`).

use std::path::Path;
use std::process::Command;

use serde::Deserialize;

use crate::session::{CheckSummary, IssueRef, PrRef};

#[derive(Debug, thiserror::Error)]
pub enum GhError {
    #[error("the GitHub CLI (gh) is not installed")]
    NotInstalled,
    #[error("not authenticated with GitHub — run `gh auth login`")]
    NotAuthenticated,
    #[error("gh failed: {0}")]
    Failed(String),
    #[error("could not parse gh output: {0}")]
    Parse(String),
}

/// Whether the `gh` CLI is available on PATH.
pub fn cli_available() -> bool {
    Command::new("gh")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Run `gh` in `repo` and return stdout, mapping common failures to [`GhError`].
fn gh(repo: &Path, args: &[&str]) -> Result<String, GhError> {
    let output = match Command::new("gh").current_dir(repo).args(args).output() {
        Ok(o) => o,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(GhError::NotInstalled),
        Err(e) => return Err(GhError::Failed(e.to_string())),
    };
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let lower = stderr.to_lowercase();
    if lower.contains("not logged") || lower.contains("authentication") || lower.contains("gh auth")
    {
        Err(GhError::NotAuthenticated)
    } else {
        Err(GhError::Failed(stderr.trim().to_string()))
    }
}

/// The `--json` field set requested for issues.
const ISSUE_FIELDS: &str = "number,title,url,state,labels";
/// The `--json` field set requested for pull requests.
const PR_FIELDS: &str = "number,title,url,state,isDraft,reviewDecision,statusCheckRollup,mergeable,mergeStateStatus,labels,headRefName";

/// Open issues for the current repo, newest first (capped at `limit`).
pub fn list_open_issues(repo: &Path, limit: u32) -> Result<Vec<IssueRef>, GhError> {
    let lim = limit.to_string();
    let json = gh(
        repo,
        &[
            "issue",
            "list",
            "--state",
            "open",
            "--limit",
            &lim,
            "--json",
            ISSUE_FIELDS,
        ],
    )?;
    parse_issues(&json).map_err(|e| GhError::Parse(e.to_string()))
}

/// Issues assigned to the authenticated user across the repo (crow's ticket board
/// is seeded by `gh search issues --assignee @me`; here we scope to the repo).
pub fn list_assigned_issues(repo: &Path, limit: u32) -> Result<Vec<IssueRef>, GhError> {
    let lim = limit.to_string();
    let json = gh(
        repo,
        &[
            "issue",
            "list",
            "--assignee",
            "@me",
            "--state",
            "open",
            "--limit",
            &lim,
            "--json",
            ISSUE_FIELDS,
        ],
    )?;
    parse_issues(&json).map_err(|e| GhError::Parse(e.to_string()))
}

/// Resolve a single issue by number or URL.
pub fn view_issue(repo: &Path, key: &str) -> Result<IssueRef, GhError> {
    let json = gh(repo, &["issue", "view", key, "--json", ISSUE_FIELDS])?;
    parse_issue(&json).map_err(|e| GhError::Parse(e.to_string()))
}

/// The pull request whose head is `branch`, if any.
pub fn pr_for_branch(repo: &Path, branch: &str) -> Result<Option<PrRef>, GhError> {
    let json = gh(
        repo,
        &[
            "pr", "list", "--head", branch, "--state", "all", "--json", PR_FIELDS,
        ],
    )?;
    let prs = parse_prs(&json).map_err(|e| GhError::Parse(e.to_string()))?;
    Ok(prs.into_iter().next())
}

/// Pull requests open against the repo and awaiting review (crow's review board).
pub fn list_review_prs(repo: &Path, limit: u32) -> Result<Vec<PrRef>, GhError> {
    let lim = limit.to_string();
    let json = gh(
        repo,
        &[
            "pr", "list", "--state", "open", "--limit", &lim, "--json", PR_FIELDS,
        ],
    )?;
    parse_prs(&json).map_err(|e| GhError::Parse(e.to_string()))
}

// ---- actions (automation & quick actions) ----------------------------------

/// Push the branch and open a PR for it, returning the new PR's URL.
pub fn create_pr(repo: &Path, branch: &str, title: &str, draft: bool) -> Result<String, GhError> {
    // Best-effort push first; ignore "everything up-to-date" style failures by
    // not treating push as fatal to PR creation when the branch already exists.
    let _ = gh(repo, &["pr", "create", "--head", branch, "--fill"]);
    let mut args = vec![
        "pr", "create", "--head", branch, "--title", title, "--body", "",
    ];
    if draft {
        args.push("--draft");
    }
    // If a PR already exists `gh pr create` errors; fall back to viewing it.
    match gh(repo, &args) {
        Ok(out) => Ok(out.trim().to_string()),
        Err(_) => view_pr_url(repo, branch),
    }
}

/// The URL of the PR for `branch`, if one exists.
pub fn view_pr_url(repo: &Path, branch: &str) -> Result<String, GhError> {
    let out = gh(repo, &["pr", "view", branch, "--json", "url", "-q", ".url"])?;
    Ok(out.trim().to_string())
}

/// Squash-merge the PR for `branch` (used by auto-merge).
pub fn merge_pr(repo: &Path, branch: &str) -> Result<(), GhError> {
    gh(repo, &["pr", "merge", branch, "--squash"]).map(|_| ())
}

/// Mark a draft PR ready for review.
pub fn mark_pr_ready(repo: &Path, branch: &str) -> Result<(), GhError> {
    gh(repo, &["pr", "ready", branch]).map(|_| ())
}

/// Add a label to an issue (used to create/apply automation labels).
pub fn add_issue_label(repo: &Path, number: u64, label: &str) -> Result<(), GhError> {
    gh(
        repo,
        &["issue", "edit", &number.to_string(), "--add-label", label],
    )
    .map(|_| ())
}

// ---- pure parsing ----------------------------------------------------------

#[derive(Deserialize, Default)]
struct Label {
    #[serde(default)]
    name: String,
}

fn label_names(labels: Vec<Label>) -> Vec<String> {
    labels
        .into_iter()
        .map(|l| l.name)
        .filter(|n| !n.is_empty())
        .collect()
}

#[derive(Deserialize, Default)]
struct IssueDto {
    number: u64,
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    state: String,
    #[serde(default)]
    labels: Vec<Label>,
}

impl From<IssueDto> for IssueRef {
    fn from(d: IssueDto) -> Self {
        IssueRef {
            number: d.number,
            title: d.title,
            url: d.url,
            state: d.state,
            labels: label_names(d.labels),
            project_status: None,
            repo: None,
        }
    }
}

/// Parse the JSON array from `gh issue list --json ...`.
pub fn parse_issues(json: &str) -> Result<Vec<IssueRef>, serde_json::Error> {
    let dtos: Vec<IssueDto> = serde_json::from_str(json)?;
    Ok(dtos.into_iter().map(IssueRef::from).collect())
}

/// Parse the JSON object from `gh issue view --json ...`.
pub fn parse_issue(json: &str) -> Result<IssueRef, serde_json::Error> {
    let dto: IssueDto = serde_json::from_str(json)?;
    Ok(dto.into())
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct PrDto {
    number: u64,
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    state: String,
    #[serde(default)]
    is_draft: bool,
    #[serde(default)]
    review_decision: Option<String>,
    #[serde(default)]
    status_check_rollup: Vec<RollupEntry>,
    #[serde(default)]
    mergeable: Option<String>,
    #[serde(default)]
    merge_state_status: Option<String>,
    #[serde(default)]
    labels: Vec<Label>,
    #[serde(default)]
    head_ref_name: Option<String>,
}

/// One CI check from `statusCheckRollup`. The shape differs between check-runs
/// (`status` + `conclusion`) and legacy status contexts (`state`), so all three
/// are optional and we classify tolerantly.
#[derive(Deserialize, Default)]
struct RollupEntry {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    conclusion: Option<String>,
    #[serde(default)]
    state: Option<String>,
}

/// Classify one rollup entry as passing / failing / pending.
fn classify(entry: &RollupEntry) -> CheckClass {
    let up = |s: &Option<String>| s.as_deref().unwrap_or("").to_ascii_uppercase();
    let conclusion = up(&entry.conclusion);
    let status = up(&entry.status);
    let state = up(&entry.state);

    let failing = [
        "FAILURE",
        "TIMED_OUT",
        "CANCELLED",
        "ACTION_REQUIRED",
        "STARTUP_FAILURE",
        "ERROR",
    ];
    let passing = ["SUCCESS", "NEUTRAL", "SKIPPED"];

    if failing.contains(&conclusion.as_str()) || failing.contains(&state.as_str()) {
        CheckClass::Failing
    } else if passing.contains(&conclusion.as_str()) || passing.contains(&state.as_str()) {
        CheckClass::Passing
    } else if status == "COMPLETED" && conclusion.is_empty() {
        // Completed with no conclusion: treat as passing.
        CheckClass::Passing
    } else {
        // IN_PROGRESS / QUEUED / PENDING / WAITING / EXPECTED / unknown.
        CheckClass::Pending
    }
}

enum CheckClass {
    Passing,
    Failing,
    Pending,
}

fn summarize(entries: &[RollupEntry]) -> CheckSummary {
    let mut s = CheckSummary::default();
    for e in entries {
        match classify(e) {
            CheckClass::Passing => s.passing += 1,
            CheckClass::Failing => s.failing += 1,
            CheckClass::Pending => s.pending += 1,
        }
    }
    s
}

impl From<PrDto> for PrRef {
    fn from(d: PrDto) -> Self {
        let checks = summarize(&d.status_check_rollup);
        PrRef {
            number: d.number,
            title: d.title,
            url: d.url,
            state: d.state,
            is_draft: d.is_draft,
            review_decision: d.review_decision.filter(|s| !s.is_empty()),
            checks,
            mergeable: d.mergeable.filter(|s| !s.is_empty()),
            merge_state_status: d.merge_state_status.filter(|s| !s.is_empty()),
            labels: label_names(d.labels),
            head: d.head_ref_name.filter(|s| !s.is_empty()),
        }
    }
}

/// Parse the JSON array from `gh pr list --json ...`.
pub fn parse_prs(json: &str) -> Result<Vec<PrRef>, serde_json::Error> {
    let dtos: Vec<PrDto> = serde_json::from_str(json)?;
    Ok(dtos.into_iter().map(PrRef::from).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_issue_list_with_labels() {
        let json = r#"[
          {"number": 12, "title": "Add OAuth", "url": "https://github.com/o/r/issues/12", "state": "OPEN",
           "labels": [{"name": "crow:auto"}, {"name": "enhancement"}]},
          {"number": 9,  "title": "Fix crash", "url": "https://github.com/o/r/issues/9",  "state": "OPEN", "labels": []}
        ]"#;
        let issues = parse_issues(json).unwrap();
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].number, 12);
        assert!(issues[0].has_label("crow:auto"));
        assert!(!issues[1].has_label("crow:auto"));
    }

    #[test]
    fn parses_pr_with_mixed_checks_and_merge_state() {
        let json = r#"[{
          "number": 42,
          "title": "Implement sessions",
          "url": "https://github.com/o/r/pull/42",
          "state": "OPEN",
          "isDraft": false,
          "reviewDecision": "CHANGES_REQUESTED",
          "mergeable": "CONFLICTING",
          "mergeStateStatus": "DIRTY",
          "labels": [{"name": "crow:merge"}],
          "statusCheckRollup": [
            {"status": "COMPLETED", "conclusion": "SUCCESS"},
            {"status": "COMPLETED", "conclusion": "FAILURE"},
            {"status": "IN_PROGRESS", "conclusion": null},
            {"state": "SUCCESS"},
            {"status": "COMPLETED", "conclusion": "SKIPPED"}
          ]
        }]"#;
        let prs = parse_prs(json).unwrap();
        let pr = &prs[0];
        assert_eq!(pr.number, 42);
        assert!(pr.is_open());
        assert_eq!(pr.review_decision.as_deref(), Some("CHANGES_REQUESTED"));
        assert!(pr.changes_requested());
        assert!(pr.is_conflicting());
        assert!(pr.has_label("crow:merge"));
        assert_eq!(pr.checks.passing, 3);
        assert_eq!(pr.checks.failing, 1);
        assert_eq!(pr.checks.pending, 1);
        assert_eq!(pr.checks.total(), 5);
    }

    #[test]
    fn empty_review_decision_becomes_none() {
        let json = r#"[{
          "number": 1, "title": "t", "url": "u", "state": "OPEN",
          "isDraft": true, "reviewDecision": "", "statusCheckRollup": []
        }]"#;
        let pr = &parse_prs(json).unwrap()[0];
        assert_eq!(pr.review_decision, None);
        assert!(pr.is_draft);
        assert_eq!(pr.checks.total(), 0);
        assert!(pr.mergeable.is_none());
    }
}
