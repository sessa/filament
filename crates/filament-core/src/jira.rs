//! Jira (task backend) integration via the Atlassian `acli` CLI.
//!
//! crow can track tasks in Jira while keeping code & PRs on GitHub/GitLab. This
//! module is the Jira analogue of issue reading: it shells out to
//! [`acli`](https://developer.atlassian.com/cloud/acli/) to list and view work
//! items and to transition their status, normalizing the result into the shared
//! [`IssueRef`] model. `acli`'s exact JSON varies by version, so the parser is
//! deliberately tolerant (it walks [`serde_json::Value`] and accepts several
//! common shapes) and every call degrades gracefully when `acli` is missing or
//! unauthenticated. Jira keys (`ACME-123`) are non-numeric, so the numeric
//! suffix becomes [`IssueRef::number`] and the full key is kept in
//! [`IssueRef::repo`].

use std::process::Command;

use serde_json::Value;

use crate::config::JiraConfig;
use crate::session::IssueRef;

#[derive(Debug, thiserror::Error)]
pub enum JiraError {
    #[error("the Atlassian CLI (acli) is not installed")]
    NotInstalled,
    #[error("not authenticated with Jira — run `acli jira auth login`")]
    NotAuthenticated,
    #[error("acli failed: {0}")]
    Failed(String),
    #[error("could not parse acli output: {0}")]
    Parse(String),
}

/// Whether the `acli` CLI is available on PATH.
pub fn cli_available() -> bool {
    Command::new("acli")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn acli(args: &[&str]) -> Result<String, JiraError> {
    let output = match Command::new("acli").args(args).output() {
        Ok(o) => o,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(JiraError::NotInstalled),
        Err(e) => return Err(JiraError::Failed(e.to_string())),
    };
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let lower = stderr.to_lowercase();
    if lower.contains("auth") || lower.contains("unauthorized") || lower.contains("401") {
        Err(JiraError::NotAuthenticated)
    } else {
        Err(JiraError::Failed(stderr.trim().to_string()))
    }
}

/// Open work items in the configured project (not in a Done status category).
pub fn list_issues(cfg: &JiraConfig, limit: u32) -> Result<Vec<IssueRef>, JiraError> {
    let jql = format!(
        "project = {} AND statusCategory != Done ORDER BY updated DESC",
        cfg.project_key
    );
    let lim = limit.to_string();
    let json = acli(&[
        "jira", "workitem", "search", "--jql", &jql, "--limit", &lim, "--json",
    ])?;
    parse_issues(&json, &cfg.site_url).map_err(|e| JiraError::Parse(e.to_string()))
}

/// Resolve a single work item by key (e.g. `ACME-123`).
pub fn view_issue(cfg: &JiraConfig, key: &str) -> Result<IssueRef, JiraError> {
    let json = acli(&["jira", "workitem", "view", "--key", key, "--json"])?;
    let val: Value = serde_json::from_str(&json).map_err(|e| JiraError::Parse(e.to_string()))?;
    issue_from_value(&val, &cfg.site_url).ok_or_else(|| JiraError::Parse("no work item".into()))
}

/// Transition a work item to a named status.
pub fn transition(_cfg: &JiraConfig, key: &str, status: &str) -> Result<(), JiraError> {
    acli(&[
        "jira",
        "workitem",
        "transition",
        "--key",
        key,
        "--status",
        status,
    ])
    .map(|_| ())
}

// ---- tolerant parsing ------------------------------------------------------

/// Parse a list of work items from `acli ... --json`, accepting either a bare
/// array or an object wrapping the array under a common key.
pub fn parse_issues(json: &str, site: &str) -> Result<Vec<IssueRef>, serde_json::Error> {
    let val: Value = serde_json::from_str(json)?;
    let arr = as_array(&val);
    Ok(arr
        .iter()
        .filter_map(|item| issue_from_value(item, site))
        .collect())
}

/// Find the array of items inside whatever envelope `acli` used.
fn as_array(val: &Value) -> Vec<Value> {
    if let Some(a) = val.as_array() {
        return a.clone();
    }
    for key in [
        "issues",
        "workItems",
        "workitems",
        "results",
        "values",
        "data",
    ] {
        if let Some(a) = val.get(key).and_then(|v| v.as_array()) {
            return a.clone();
        }
    }
    Vec::new()
}

/// Pull a string out of `val[key]` or `val["fields"][key]`, possibly nested.
fn field_str(val: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(s) = val.get(key).and_then(|v| v.as_str()) {
            return Some(s.to_string());
        }
        if let Some(s) = val
            .get("fields")
            .and_then(|f| f.get(key))
            .and_then(|v| v.as_str())
        {
            return Some(s.to_string());
        }
    }
    None
}

/// Status names live in several shapes: `status` (string), `status.name`, or
/// `fields.status.name`.
fn status_name(val: &Value) -> Option<String> {
    if let Some(s) = field_str(val, &["status"]) {
        return Some(s);
    }
    for base in [
        val.get("status"),
        val.get("fields").and_then(|f| f.get("status")),
    ] {
        if let Some(name) = base.and_then(|s| s.get("name")).and_then(|v| v.as_str()) {
            return Some(name.to_string());
        }
    }
    None
}

fn issue_from_value(val: &Value, site: &str) -> Option<IssueRef> {
    let key = field_str(val, &["key", "issueKey"])?;
    let title = field_str(val, &["summary", "title"]).unwrap_or_default();
    let status = status_name(val);
    let number = key
        .rsplit('-')
        .next()
        .and_then(|n| n.parse::<u64>().ok())
        .unwrap_or(0);
    let url = field_str(val, &["url", "self", "webUrl"]).unwrap_or_else(|| {
        let base = site.trim_end_matches('/');
        if base.is_empty() {
            key.clone()
        } else {
            format!("{base}/browse/{key}")
        }
    });
    // A "Done" status category maps to CLOSED; everything else is OPEN.
    let closed = status
        .as_deref()
        .map(|s| {
            let n: String = s.chars().filter(|c| c.is_alphanumeric()).collect();
            matches!(
                n.to_ascii_lowercase().as_str(),
                "done" | "closed" | "resolved"
            )
        })
        .unwrap_or(false);
    Some(IssueRef {
        number,
        title,
        url,
        state: if closed {
            "CLOSED".into()
        } else {
            "OPEN".into()
        },
        labels: Vec::new(),
        project_status: status,
        repo: Some(key),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_flat_array() {
        let json = r#"[
          {"key": "ACME-12", "summary": "Build login", "status": "In Progress"},
          {"key": "ACME-13", "summary": "Fix bug", "status": "Done"}
        ]"#;
        let issues = parse_issues(json, "https://acme.atlassian.net").unwrap();
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].number, 12);
        assert_eq!(issues[0].repo.as_deref(), Some("ACME-12"));
        assert_eq!(issues[0].url, "https://acme.atlassian.net/browse/ACME-12");
        assert!(!issues[0].is_closed());
        assert_eq!(issues[0].project_status.as_deref(), Some("In Progress"));
        assert!(issues[1].is_closed());
    }

    #[test]
    fn parses_nested_fields_and_envelope() {
        let json = r#"{"issues": [
          {"key": "OPS-7", "fields": {"summary": "Rotate keys", "status": {"name": "Code Review"}}}
        ]}"#;
        let issues = parse_issues(json, "https://x.atlassian.net/").unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].number, 7);
        assert_eq!(issues[0].title, "Rotate keys");
        assert_eq!(issues[0].project_status.as_deref(), Some("Code Review"));
    }
}
