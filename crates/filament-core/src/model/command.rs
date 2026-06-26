//! Slash-command frontmatter (`commands/*.md`). Commands frequently have *no*
//! frontmatter at all (the whole file is the prompt body), and the command name
//! is conventionally the filename, so every field here is optional.

use serde::Deserialize;

use super::model_id::ModelChoice;
use super::tools::ToolList;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CommandFrontmatter {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default, rename = "argument-hint", alias = "argumentHint")]
    pub argument_hint: Option<String>,
    #[serde(default, rename = "allowed-tools", alias = "allowedTools")]
    pub allowed_tools: Option<ToolList>,
    #[serde(default)]
    pub model: ModelChoice,
}

impl CommandFrontmatter {
    pub fn template(name: &str) -> String {
        format!(
            "---\ndescription: What /{name} does.\nargument-hint: \"[args]\"\n---\n\nWrite the command prompt here. Use $ARGUMENTS to interpolate input.\n"
        )
    }
}
