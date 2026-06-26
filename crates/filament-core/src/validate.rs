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
    use crate::model::ToolList;

    fn agent(name: &str, description: &str) -> AgentFrontmatter {
        // Round-trip through the deserializer so we exercise the real shape.
        serde_norway::from_str(&format!(
            "name: {name}\ndescription: {description}\ntools: Read\n"
        ))
        .unwrap()
    }

    fn skill(name: &str, description: &str) -> SkillFrontmatter {
        serde_norway::from_str(&format!("name: {name}\ndescription: {description}\n")).unwrap()
    }

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

    #[test]
    fn valid_agent_has_no_errors() {
        let report = validate_agent(&agent("ok", "A fine agent."));
        assert!(report.is_ok());
        assert!(report.errors.is_empty());
        // It declares tools, so there should be no "inherits all tools" warning.
        assert!(report.warnings.is_empty());
    }

    #[test]
    fn agent_missing_name_and_description_reports_both() {
        let mut fm = agent("placeholder", "placeholder");
        fm.name = String::new();
        fm.description = "   ".to_string();
        let report = validate_agent(&fm);
        assert!(!report.is_ok());
        assert_eq!(report.error_for("name"), Some("Name is required."));
        assert_eq!(
            report.error_for("description"),
            Some("Description is required.")
        );
    }

    #[test]
    fn agent_with_bad_name_is_an_error_not_a_panic() {
        let mut fm = agent("good", "desc");
        fm.name = "Bad Name".to_string();
        let report = validate_agent(&fm);
        assert!(!report.is_ok());
        assert!(report.error_for("name").unwrap().contains("lowercase"));
    }

    #[test]
    fn agent_without_any_tools_gets_an_inherit_warning() {
        let mut fm = agent("loose", "desc");
        fm.tools = None;
        fm.disallowed_tools = None;
        let report = validate_agent(&fm);
        // A warning, not an error: the file is still saveable.
        assert!(report.is_ok());
        assert_eq!(report.warnings.len(), 1);
        assert_eq!(report.warnings[0].field, "tools");
    }

    #[test]
    fn declaring_only_disallowed_tools_suppresses_the_warning() {
        let mut fm = agent("loose", "desc");
        fm.tools = None;
        fm.disallowed_tools = Some(ToolList::default());
        let report = validate_agent(&fm);
        assert!(report.warnings.is_empty());
    }

    #[test]
    fn skill_requires_name_and_description() {
        let mut fm = skill("ok", "fine");
        assert!(validate_skill(&fm).is_ok());

        fm.name = "Bad".to_string();
        fm.description = String::new();
        let report = validate_skill(&fm);
        assert!(report.error_for("name").unwrap().contains("lowercase"));
        assert_eq!(
            report.error_for("description"),
            Some("Description is required.")
        );
    }

    #[test]
    fn error_for_unknown_field_is_none() {
        let report = validate_agent(&agent("ok", "desc"));
        assert!(report.error_for("nonexistent").is_none());
    }
}
