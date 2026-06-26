//! Lightweight, dependency-free validation used to gate saves and surface inline
//! errors in the editor.

use crate::model::{AgentFrontmatter, SkillFrontmatter};

#[derive(Debug, Clone, Default)]
pub struct ValidationReport {
    pub errors: Vec<FieldIssue>,
    pub warnings: Vec<FieldIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldIssue {
    pub field: String,
    pub message: String,
}

impl FieldIssue {
    fn new(field: &str, message: impl Into<String>) -> FieldIssue {
        FieldIssue {
            field: field.to_string(),
            message: message.into(),
        }
    }
}

impl ValidationReport {
    /// True when there are no hard errors (warnings are allowed).
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }

    /// The first error for a given field, if any (for inline display).
    pub fn error_for(&self, field: &str) -> Option<&str> {
        self.errors
            .iter()
            .find(|e| e.field == field)
            .map(|e| e.message.as_str())
    }
}

/// A name must match `^[a-z][a-z0-9-]*$` — lowercase letters, digits, hyphens,
/// starting with a letter.
pub fn is_valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_lowercase() {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

pub fn validate_agent(fm: &AgentFrontmatter) -> ValidationReport {
    let mut report = ValidationReport::default();
    if fm.name.trim().is_empty() {
        report
            .errors
            .push(FieldIssue::new("name", "Name is required."));
    } else if !is_valid_name(&fm.name) {
        report.errors.push(FieldIssue::new(
            "name",
            "Name must be lowercase letters, digits, or hyphens, starting with a letter.",
        ));
    }
    if fm.description.trim().is_empty() {
        report
            .errors
            .push(FieldIssue::new("description", "Description is required."));
    }
    if fm.tools.is_none() && fm.disallowed_tools.is_none() {
        report.warnings.push(FieldIssue::new(
            "tools",
            "No tools specified — the agent will inherit all tools from the main conversation.",
        ));
    }
    report
}

pub fn validate_skill(fm: &SkillFrontmatter) -> ValidationReport {
    let mut report = ValidationReport::default();
    if fm.name.trim().is_empty() {
        report
            .errors
            .push(FieldIssue::new("name", "Name is required."));
    } else if !is_valid_name(&fm.name) {
        report.errors.push(FieldIssue::new(
            "name",
            "Name must be lowercase letters, digits, or hyphens, starting with a letter.",
        ));
    }
    if fm.description.trim().is_empty() {
        report
            .errors
            .push(FieldIssue::new("description", "Description is required."));
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_rules() {
        assert!(is_valid_name("code-reviewer"));
        assert!(is_valid_name("a1"));
        assert!(!is_valid_name("Code"));
        assert!(!is_valid_name("1abc"));
        assert!(!is_valid_name("has space"));
        assert!(!is_valid_name(""));
        assert!(!is_valid_name("-leading"));
    }
}
