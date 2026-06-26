//! `model` field — an alias (`sonnet`/`opus`/`haiku`/`fable`), `inherit`, or a
//! full model id like `claude-opus-4-8`. Unknown strings are preserved verbatim
//! as [`ModelChoice::Full`] so we never lose or mangle a value we don't
//! recognise.

use serde::Deserialize;

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(from = "String")]
pub enum ModelChoice {
    /// Use the main conversation's model (the default when the field is absent).
    #[default]
    Inherit,
    Sonnet,
    Opus,
    Haiku,
    Fable,
    /// An explicit, unrecognised model id, kept exactly as written.
    Full(String),
}

impl From<String> for ModelChoice {
    fn from(s: String) -> Self {
        match s.as_str() {
            "inherit" => ModelChoice::Inherit,
            "sonnet" => ModelChoice::Sonnet,
            "opus" => ModelChoice::Opus,
            "haiku" => ModelChoice::Haiku,
            "fable" => ModelChoice::Fable,
            _ => ModelChoice::Full(s),
        }
    }
}

impl ModelChoice {
    /// The known aliases, for dropdowns. `Full(_)` is represented separately in
    /// the editor via a free-text field.
    pub const ALIASES: [ModelChoice; 5] = [
        ModelChoice::Inherit,
        ModelChoice::Sonnet,
        ModelChoice::Opus,
        ModelChoice::Haiku,
        ModelChoice::Fable,
    ];

    /// The string as it should be written to YAML.
    pub fn as_str(&self) -> &str {
        match self {
            ModelChoice::Inherit => "inherit",
            ModelChoice::Sonnet => "sonnet",
            ModelChoice::Opus => "opus",
            ModelChoice::Haiku => "haiku",
            ModelChoice::Fable => "fable",
            ModelChoice::Full(s) => s,
        }
    }

    /// A friendly label for badges (`Full` ids shown as-is).
    pub fn label(&self) -> String {
        match self {
            ModelChoice::Inherit => "inherit".to_string(),
            ModelChoice::Sonnet => "Sonnet".to_string(),
            ModelChoice::Opus => "Opus".to_string(),
            ModelChoice::Haiku => "Haiku".to_string(),
            ModelChoice::Fable => "Fable".to_string(),
            ModelChoice::Full(s) => s.clone(),
        }
    }

    pub fn is_inherit(&self) -> bool {
        matches!(self, ModelChoice::Inherit)
    }
}

impl std::fmt::Display for ModelChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases_and_full() {
        assert_eq!(ModelChoice::from("opus".to_string()), ModelChoice::Opus);
        assert_eq!(
            ModelChoice::from("inherit".to_string()),
            ModelChoice::Inherit
        );
        assert_eq!(
            ModelChoice::from("claude-opus-4-8".to_string()),
            ModelChoice::Full("claude-opus-4-8".to_string())
        );
        assert_eq!(ModelChoice::Full("x".into()).as_str(), "x");
    }
}
