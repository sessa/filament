//! Provider-agnostic facade over [`crate::github`], [`crate::gitlab`], and
//! [`crate::jira`].
//!
//! The UI and automation talk to *one* surface here and dispatch on the session's
//! [`CodeProvider`] / [`TaskProvider`]. A single [`ProviderError`] unifies the
//! three backends' error types so the same "not installed / not authenticated /
//! failed" hint logic works regardless of forge or tracker.

use std::path::Path;

use crate::backend::{CodeProvider, TaskProvider};
use crate::config::JiraConfig;
use crate::session::{IssueRef, PrRef};
use crate::{github, gitlab, jira};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderError {
    NotInstalled,
    NotAuthenticated,
    Failed(String),
    Parse(String),
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderError::NotInstalled => f.write_str("CLI not installed"),
            ProviderError::NotAuthenticated => f.write_str("not authenticated"),
            ProviderError::Failed(s) => write!(f, "{s}"),
            ProviderError::Parse(s) => write!(f, "parse error: {s}"),
        }
    }
}

impl From<github::GhError> for ProviderError {
    fn from(e: github::GhError) -> Self {
        match e {
            github::GhError::NotInstalled => ProviderError::NotInstalled,
            github::GhError::NotAuthenticated => ProviderError::NotAuthenticated,
            github::GhError::Failed(s) => ProviderError::Failed(s),
            github::GhError::Parse(s) => ProviderError::Parse(s),
        }
    }
}

impl From<gitlab::GlError> for ProviderError {
    fn from(e: gitlab::GlError) -> Self {
        match e {
            gitlab::GlError::NotInstalled => ProviderError::NotInstalled,
            gitlab::GlError::NotAuthenticated => ProviderError::NotAuthenticated,
            gitlab::GlError::Failed(s) => ProviderError::Failed(s),
            gitlab::GlError::Parse(s) => ProviderError::Parse(s),
        }
    }
}

impl From<jira::JiraError> for ProviderError {
    fn from(e: jira::JiraError) -> Self {
        match e {
            jira::JiraError::NotInstalled => ProviderError::NotInstalled,
            jira::JiraError::NotAuthenticated => ProviderError::NotAuthenticated,
            jira::JiraError::Failed(s) => ProviderError::Failed(s),
            jira::JiraError::Parse(s) => ProviderError::Parse(s),
        }
    }
}

/// Whether the CLI backing `provider` is on PATH.
pub fn code_cli_available(provider: CodeProvider) -> bool {
    match provider {
        CodeProvider::GitHub => github::cli_available(),
        CodeProvider::GitLab => gitlab::cli_available(),
    }
}

/// Whether the CLI backing the task `provider` is on PATH.
pub fn task_cli_available(provider: TaskProvider) -> bool {
    match provider {
        TaskProvider::GitHub => github::cli_available(),
        TaskProvider::GitLab => gitlab::cli_available(),
        TaskProvider::Jira => jira::cli_available(),
    }
}

// ---- tasks (issues / tickets) ----------------------------------------------

/// Open tickets for the active board, from the configured task backend.
pub fn list_open_issues(
    task: TaskProvider,
    repo: &Path,
    host: Option<&str>,
    jira_cfg: &JiraConfig,
    limit: u32,
) -> Result<Vec<IssueRef>, ProviderError> {
    match task {
        TaskProvider::GitHub => Ok(github::list_open_issues(repo, limit)?),
        TaskProvider::GitLab => Ok(gitlab::list_open_issues(repo, host, limit)?),
        TaskProvider::Jira => Ok(jira::list_issues(jira_cfg, limit)?),
    }
}

/// Resolve a single ticket by key/number/URL from the task backend.
pub fn view_issue(
    task: TaskProvider,
    repo: &Path,
    host: Option<&str>,
    jira_cfg: &JiraConfig,
    key: &str,
) -> Result<IssueRef, ProviderError> {
    match task {
        TaskProvider::GitHub => Ok(github::view_issue(repo, key)?),
        TaskProvider::GitLab => Ok(gitlab::view_issue(repo, host, key)?),
        TaskProvider::Jira => Ok(jira::view_issue(jira_cfg, key)?),
    }
}

// ---- code (PRs / MRs) ------------------------------------------------------

/// The change request (PR/MR) for `branch`, if any.
pub fn pr_for_branch(
    code: CodeProvider,
    repo: &Path,
    host: Option<&str>,
    branch: &str,
) -> Result<Option<PrRef>, ProviderError> {
    match code {
        CodeProvider::GitHub => Ok(github::pr_for_branch(repo, branch)?),
        CodeProvider::GitLab => Ok(gitlab::mr_for_branch(repo, host, branch)?),
    }
}

/// Open change requests for the project (review board).
pub fn list_review_prs(
    code: CodeProvider,
    repo: &Path,
    host: Option<&str>,
    limit: u32,
) -> Result<Vec<PrRef>, ProviderError> {
    match code {
        CodeProvider::GitHub => Ok(github::list_review_prs(repo, limit)?),
        CodeProvider::GitLab => Ok(gitlab::list_review_mrs(repo, host, limit)?),
    }
}

/// Open a change request for `branch`, returning its URL.
pub fn create_pr(
    code: CodeProvider,
    repo: &Path,
    host: Option<&str>,
    branch: &str,
    title: &str,
    draft: bool,
) -> Result<String, ProviderError> {
    match code {
        CodeProvider::GitHub => Ok(github::create_pr(repo, branch, title, draft)?),
        CodeProvider::GitLab => Ok(gitlab::create_mr(repo, host, branch, title)?),
    }
}

/// Squash-merge the change request for `branch`.
pub fn merge_pr(
    code: CodeProvider,
    repo: &Path,
    host: Option<&str>,
    branch: &str,
) -> Result<(), ProviderError> {
    match code {
        CodeProvider::GitHub => Ok(github::merge_pr(repo, branch)?),
        CodeProvider::GitLab => Ok(gitlab::merge_mr(repo, host, branch)?),
    }
}

/// Mark a draft change request ready for review.
pub fn mark_pr_ready(
    code: CodeProvider,
    repo: &Path,
    host: Option<&str>,
    branch: &str,
) -> Result<(), ProviderError> {
    match code {
        CodeProvider::GitHub => Ok(github::mark_pr_ready(repo, branch)?),
        CodeProvider::GitLab => Ok(gitlab::mark_mr_ready(repo, host, branch)?),
    }
}
