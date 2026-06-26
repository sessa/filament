//! Skill frontmatter (`SKILL.md`). Like agents, the body is loaded on demand
//! when the skill is invoked. `name` and `description` are required.

use serde::Deserialize;
use serde_norway::Value as Yaml;

use super::enums::Effort;
use super::model_id::ModelChoice;
use super::tools::ToolList;

#[derive(Debug, Clone, Deserialize)]
pub struct SkillFrontmatter {
    pub name: String,
    pub description: String,

    #[serde(default, alias = "whenToUse", alias = "when-to-use")]
    pub when_to_use: Option<String>,
    #[serde(
        default,
        rename = "disable-model-invocation",
        alias = "disableModelInvocation"
    )]
    pub disable_model_invocation: Option<bool>,

    #[serde(default, rename = "allowed-tools", alias = "allowedTools")]
    pub allowed_tools: Option<ToolList>,
    #[serde(default, rename = "disallowed-tools", alias = "disallowedTools")]
    pub disallowed_tools: Option<ToolList>,

    #[serde(default)]
    pub model: ModelChoice,
    #[serde(default)]
    pub effort: Option<Effort>,
    /// e.g. `fork` to run in a forked context.
    #[serde(default)]
    pub context: Option<String>,
    /// The agent a skill runs under, if pinned.
    #[serde(default)]
    pub agent: Option<String>,

    #[serde(default)]
    pub arguments: Option<Yaml>,
    #[serde(default, rename = "argument-hint", alias = "argumentHint")]
    pub argument_hint: Option<String>,
    #[serde(default)]
    pub paths: Option<Yaml>,
    #[serde(default)]
    pub shell: Option<Yaml>,
    #[serde(default)]
    pub hooks: Option<Yaml>,
}

impl SkillFrontmatter {
    pub fn template(name: &str) -> String {
        format!(
            "---\nname: {name}\ndescription: What this skill does and when to use it.\n---\n\n# {name}\n\nDescribe the procedure here.\n"
        )
    }
}
