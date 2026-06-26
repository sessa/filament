//! `settings.json` — permissions, hooks, env, and a default agent. Unknown keys
//! are preserved in `extra` so a re-serialize never drops data.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value as Json;

use super::hook::{parse_hooks, HookEventGroup};

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub permissions: Permissions,
    #[serde(default)]
    pub hooks: Option<Json>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default, rename = "skillOverrides")]
    pub skill_overrides: Option<Json>,
    /// Everything we don't model explicitly (`$schema`, `model`, etc.).
    #[serde(flatten)]
    pub extra: BTreeMap<String, Json>,
}

impl Settings {
    /// Structured view of the hooks block for display.
    pub fn hook_groups(&self) -> Vec<HookEventGroup> {
        self.hooks.as_ref().map(parse_hooks).unwrap_or_default()
    }

    pub fn template() -> String {
        "{\n  \"permissions\": {\n    \"allow\": [],\n    \"deny\": []\n  }\n}\n".to_string()
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Permissions {
    #[serde(default)]
    pub allow: Vec<String>,
    #[serde(default)]
    pub deny: Vec<String>,
    #[serde(default)]
    pub ask: Vec<String>,
}

impl Permissions {
    pub fn is_empty(&self) -> bool {
        self.allow.is_empty() && self.deny.is_empty() && self.ask.is_empty()
    }
}
