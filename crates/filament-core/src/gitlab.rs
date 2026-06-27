//! GitLab integration via the `glab` CLI.
//!
//! The GitLab analogue of [`crate::github`]: it shells out to the user's
//! authenticated [`glab`](https://gitlab.com/gitlab-org/cli) to read issues and
//! merge-request status. `glab` is **optional** and every call returns a typed
//! [`GlError`] so the UI degrades quietly. GitLab's JSON (snake_case, REST shape)
//! is normalized into the same [`IssueRef`] / [`PrRef`] model used for GitHub, so
//! the rest of Filament is provider-agnostic. Self-hosted instances are reached
//! by setting `GITLAB_HOST` from the workspace `host`. Pure parsers are
//! unit-tested against representative GitLab JSON.

use std::path::Path;
use std::process::Command;

use serde::Deserialize;

use crate::session::{CheckSummary, IssueRef, PrRef};

#[derive(Debug, thiserror::Error)]
pub enum GlError {
    #[error("the GitLab CLI (glab) is not installed")]
    NotInstalled,
    #[error("not authenticated with GitLab — run `glab auth login`")]
    NotAuthenticated,
    #[error("glab failed: {0}")]
    Failed(String),
    #[error("could not parse glab output: {0}")]
    Parse(String),
}

/// Whether the `glab` CLI is available on PATH.
pub fn cli_available() -> bool {
    Command::new("glab")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Run `glab` in `repo` (with `GITLAB_HOST` set when `host` is given) and return
/// stdout, mapping common failures to [`GlError`].
fn glab(repo: &Path, host: Option<&str>, args: &[&str]) -> Result<String, GlError> {
    let mut cmd = Command::new("glab");
    cmd.current_dir(repo).args(args);
    if let Some(h) = host.filter(|h| !h.is_empty()) {
        cmd.env("GITLAB_HOST", h);
    }
    let output = match cmd.output() {
        Ok(o) => o,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(GlError::NotInstalled),
        Err(e) => return Err(GlError::Failed(e.to_string())),
    };
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let lower = stderr.to_lowercase();
    if lower.contains("not authenticated") || lower.contains("auth login") || lower.contains("401")
    {
        Err(GlError::NotAuthenticated)
    } else {
        Err(GlError::Failed(stderr.trim().to_string()))
    }
}

/// Open issues for the current project, capped at `limit`.
pub fn list_open_issues(
    repo: &Path,
    host: Option<&str>,
    limit: u32,
) -> Result<Vec<IssueRef>, GlError> {
    let lim = limit.to_string();
    let json = glab(
        repo,
        host,
        &["issue", "list", "--per-page", &lim, "-F", "json"],
    )?;
    parse_issues(&json).map_err(|e| GlError::Parse(e.to_string()))
}

/// Open issues assigned to the authenticated user.
pub fn list_assigned_issues(
    repo: &Path,
    host: Option<&str>,
    limit: u32,
) -> Result<Vec<IssueRef>, GlError> {
    let lim = limit.to_string();
    let json = glab(
        repo,
        host,
        &[
            "issue",
            "list",
            "--assignee",
            "@me",
            "--per-page",
            &lim,
            "-F",
            "json",
        ],
    )?;
    parse_issues(&json).map_err(|e| GlError::Parse(e.to_string()))
}

/// Resolve a single issue by IID or URL.
pub fn view_issue(repo: &Path, host: Option<&str>, key: &str) -> Result<IssueRef, GlError> {
    let json = glab(repo, host, &["issue", "view", key, "-F", "json"])?;
    parse_issue(&json).map_err(|e| GlError::Parse(e.to_string()))
}

/// The merge request whose source branch is `branch`, if any.
pub fn mr_for_branch(
    repo: &Path,
    host: Option<&str>,
    branch: &str,
) -> Result<Option<PrRef>, GlError> {
    let json = glab(
        repo,
        host,
        &[
            "mr",
            "list",
            "--source-branch",
            branch,
            "--all",
            "-F",
            "json",
        ],
    )?;
    let mrs = parse_mrs(&json).map_err(|e| GlError::Parse(e.to_string()))?;
    Ok(mrs.into_iter().next())
}

/// Open merge requests for the project (review board).
pub fn list_review_mrs(repo: &Path, host: Option<&str>, limit: u32) -> Result<Vec<PrRef>, GlError> {
    let lim = limit.to_string();
    let json = glab(
        repo,
        host,
        &["mr", "list", "--per-page", &lim, "-F", "json"],
    )?;
    parse_mrs(&json).map_err(|e| GlError::Parse(e.to_string()))
}

// ---- actions ---------------------------------------------------------------

/// Create a merge request for `branch`, returning its web URL (best effort).
pub fn create_mr(
    repo: &Path,
    host: Option<&str>,
    branch: &str,
    title: &str,
) -> Result<String, GlError> {
    let out = glab(
        repo,
        host,
        &[
            "mr",
            "create",
            "--source-branch",
            branch,
            "--title",
            title,
            "--fill",
            "--yes",
        ],
    )?;
    Ok(out.trim().to_string())
}

/// Squash-merge the MR for `branch`.
pub fn merge_mr(repo: &Path, host: Option<&str>, branch: &str) -> Result<(), GlError> {
    glab(repo, host, &["mr", "merge", branch, "--squash", "--yes"]).map(|_| ())
}

/// Mark a draft MR ready.
pub fn mark_mr_ready(repo: &Path, host: Option<&str>, branch: &str) -> Result<(), GlError> {
    glab(repo, host, &["mr", "update", branch, "--ready"]).map(|_| ())
}

// ---- pure parsing ----------------------------------------------------------

fn norm_issue_state(s: &str) -> String {
    match s.to_ascii_lowercase().as_str() {
        "opened" | "open" => "OPEN".into(),
        "closed" => "CLOSED".into(),
        other => other.to_ascii_uppercase(),
    }
}

fn norm_mr_state(s: &str) -> String {
    match s.to_ascii_lowercase().as_str() {
        "opened" | "open" => "OPEN".into(),
        "merged" => "MERGED".into(),
        "closed" | "locked" => "CLOSED".into(),
        other => other.to_ascii_uppercase(),
    }
}

#[derive(Deserialize, Default)]
struct IssueDto {
    #[serde(default)]
    iid: u64,
    #[serde(default)]
    title: String,
    #[serde(default)]
    web_url: String,
    #[serde(default)]
    state: String,
    #[serde(default)]
    labels: Vec<String>,
}

impl From<IssueDto> for IssueRef {
    fn from(d: IssueDto) -> Self {
        IssueRef {
            number: d.iid,
            title: d.title,
            url: d.web_url,
            state: norm_issue_state(&d.state),
            labels: d.labels,
            project_status: None,
            repo: None,
        }
    }
}

pub fn parse_issues(json: &str) -> Result<Vec<IssueRef>, serde_json::Error> {
    let dtos: Vec<IssueDto> = serde_json::from_str(json)?;
    Ok(dtos.into_iter().map(IssueRef::from).collect())
}

pub fn parse_issue(json: &str) -> Result<IssueRef, serde_json::Error> {
    let dto: IssueDto = serde_json::from_str(json)?;
    Ok(dto.into())
}

#[derive(Deserialize, Default)]
struct Pipeline {
    #[serde(default)]
    status: String,
}

#[derive(Deserialize, Default)]
struct MrDto {
    #[serde(default)]
    iid: u64,
    #[serde(default)]
    title: String,
    #[serde(default)]
    web_url: String,
    #[serde(default)]
    state: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    work_in_progress: bool,
    #[serde(default)]
    merge_status: String,
    #[serde(default)]
    detailed_merge_status: String,
    #[serde(default)]
    labels: Vec<String>,
    #[serde(default)]
    pipeline: Option<Pipeline>,
    #[serde(default)]
    head_pipeline: Option<Pipeline>,
    #[serde(default)]
    source_branch: String,
}

fn pipeline_checks(mr: &MrDto) -> CheckSummary {
    let status = mr
        .head_pipeline
        .as_ref()
        .or(mr.pipeline.as_ref())
        .map(|p| p.status.to_ascii_lowercase());
    let mut s = CheckSummary::default();
    match status.as_deref() {
        Some("success" | "passed") => s.passing = 1,
        Some("failed" | "canceled" | "cancelled") => s.failing = 1,
        Some("running" | "pending" | "created" | "waiting_for_resource" | "preparing") => {
            s.pending = 1
        }
        _ => {}
    }
    s
}

fn norm_mergeable(merge_status: &str) -> Option<String> {
    match merge_status.to_ascii_lowercase().as_str() {
        "can_be_merged" => Some("MERGEABLE".into()),
        "cannot_be_merged" | "cannot_be_merged_recheck" => Some("CONFLICTING".into()),
        "" => None,
        _ => Some("UNKNOWN".into()),
    }
}

impl From<MrDto> for PrRef {
    fn from(d: MrDto) -> Self {
        let checks = pipeline_checks(&d);
        let mergeable = norm_mergeable(&d.merge_status);
        PrRef {
            number: d.iid,
            title: d.title,
            url: d.web_url,
            state: norm_mr_state(&d.state),
            is_draft: d.draft || d.work_in_progress,
            review_decision: None,
            checks,
            mergeable,
            merge_state_status: (!d.detailed_merge_status.is_empty())
                .then(|| d.detailed_merge_status.to_ascii_uppercase()),
            labels: d.labels,
            head: (!d.source_branch.is_empty()).then(|| d.source_branch.clone()),
        }
    }
}

pub fn parse_mrs(json: &str) -> Result<Vec<PrRef>, serde_json::Error> {
    let dtos: Vec<MrDto> = serde_json::from_str(json)?;
    Ok(dtos.into_iter().map(PrRef::from).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gitlab_issues() {
        let json = r#"[
          {"iid": 7, "title": "Add login", "web_url": "https://gitlab.com/g/p/-/issues/7",
           "state": "opened", "labels": ["crow:auto", "backend"]}
        ]"#;
        let issues = parse_issues(json).unwrap();
        assert_eq!(issues[0].number, 7);
        assert_eq!(issues[0].state, "OPEN");
        assert!(issues[0].has_label("crow:auto"));
    }

    #[test]
    fn parses_gitlab_mrs() {
        let json = r#"[{
          "iid": 21, "title": "Implement login", "web_url": "https://gitlab.com/g/p/-/merge_requests/21",
          "state": "opened", "draft": false, "merge_status": "cannot_be_merged",
          "detailed_merge_status": "conflict", "labels": ["crow:merge"],
          "head_pipeline": {"status": "failed"}
        }]"#;
        let mrs = parse_mrs(json).unwrap();
        let mr = &mrs[0];
        assert_eq!(mr.number, 21);
        assert!(mr.is_open());
        assert!(mr.is_conflicting());
        assert_eq!(mr.checks.failing, 1);
        assert!(mr.has_label("crow:merge"));
    }
}
