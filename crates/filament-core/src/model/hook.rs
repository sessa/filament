//! Structured view of the `hooks` object found in `settings.json` (and, opaque,
//! in agent/skill frontmatter).
//!
//! Shape:
//! ```json
//! { "PreToolUse": [ { "matcher": "Bash", "hooks": [ {"type":"command","command":"…"} ] } ] }
//! ```
//! We parse defensively — anything that doesn't fit is simply skipped, never an
//! error.

use serde_json::Value as Json;

/// The canonical hook event names, in display order.
pub const HOOK_EVENTS: [&str; 8] = [
    "SessionStart",
    "SessionEnd",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "Stop",
    "SubagentStart",
    "SubagentStop",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookEventGroup {
    pub event: String,
    pub matchers: Vec<HookMatcher>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookMatcher {
    /// The tool/pattern this group matches (absent for events without matchers).
    pub matcher: Option<String>,
    pub commands: Vec<HookCommand>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookCommand {
    /// Usually `command`.
    pub kind: String,
    pub command: String,
}

/// Parse a `hooks` JSON object into structured groups, in [`HOOK_EVENTS`] order
/// (with any unrecognised events appended).
pub fn parse_hooks(hooks: &Json) -> Vec<HookEventGroup> {
    let Some(obj) = hooks.as_object() else {
        return Vec::new();
    };

    let mut groups: Vec<HookEventGroup> = Vec::new();
    let push_event = |event: &str, groups: &mut Vec<HookEventGroup>| {
        if let Some(arr) = obj.get(event).and_then(Json::as_array) {
            let matchers = arr.iter().filter_map(parse_matcher).collect::<Vec<_>>();
            if !matchers.is_empty() {
                groups.push(HookEventGroup {
                    event: event.to_string(),
                    matchers,
                });
            }
        }
    };

    for event in HOOK_EVENTS {
        push_event(event, &mut groups);
    }
    // Any events we don't have in the canonical list.
    for key in obj.keys() {
        if !HOOK_EVENTS.contains(&key.as_str()) {
            push_event(key, &mut groups);
        }
    }
    groups
}

fn parse_matcher(v: &Json) -> Option<HookMatcher> {
    let obj = v.as_object()?;
    let matcher = obj
        .get("matcher")
        .and_then(Json::as_str)
        .map(str::to_string);
    let commands = obj
        .get("hooks")
        .and_then(Json::as_array)
        .map(|a| a.iter().filter_map(parse_command).collect())
        .unwrap_or_default();
    Some(HookMatcher { matcher, commands })
}

fn parse_command(v: &Json) -> Option<HookCommand> {
    let obj = v.as_object()?;
    let kind = obj
        .get("type")
        .and_then(Json::as_str)
        .unwrap_or("command")
        .to_string();
    let command = obj.get("command").and_then(Json::as_str)?.to_string();
    Some(HookCommand { kind, command })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_grouped_hooks() {
        let v = serde_json::json!({
            "PreToolUse": [
                { "matcher": "Bash", "hooks": [ {"type":"command","command":"echo hi"} ] }
            ],
            "Stop": [ { "hooks": [ {"command":"cleanup.sh"} ] } ]
        });
        let g = parse_hooks(&v);
        assert_eq!(g.len(), 2);
        assert_eq!(g[0].event, "PreToolUse");
        assert_eq!(g[0].matchers[0].matcher.as_deref(), Some("Bash"));
        assert_eq!(g[0].matchers[0].commands[0].command, "echo hi");
        assert_eq!(g[1].event, "Stop");
        assert_eq!(g[1].matchers[0].commands[0].kind, "command");
    }
}
