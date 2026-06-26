//! Tool allow/deny lists.
//!
//! On disk `tools` / `disallowedTools` may be a YAML sequence *or* a single
//! string with comma/space-separated entries. Entries are not just plain names:
//! they can be MCP patterns (`mcp__server`, `mcp__server__tool`, `mcp__server__*`),
//! nested-agent grants (`Agent(worker, researcher)` — note the inner commas), or
//! the bare `Skill` token. [`ToolSpec`] turns each entry into something the UI
//! can render as a typed chip, and the parsing is round-trippable via
//! [`ToolSpec::to_token`].

use std::fmt;

use serde::de::{self, Deserializer, SeqAccess, Visitor};
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolSpec {
    /// A built-in tool such as `Read`, `Edit`, `Bash`.
    Builtin(String),
    /// An MCP grant. `tool == None` means the whole server (`mcp__server`) or an
    /// explicit wildcard (`mcp__server__*`).
    Mcp {
        server: String,
        tool: Option<String>,
    },
    /// `Agent(a, b)` allowing nested spawning of the listed subagents. An empty
    /// vec means a bare `Agent` (any subagent).
    Agent(Vec<String>),
    /// The bare `Skill` token (lets the agent load skills).
    Skill,
    /// Anything we don't specifically recognise; preserved verbatim.
    Other(String),
}

impl ToolSpec {
    /// Parse a single token (already split out of a list).
    pub fn parse(token: &str) -> ToolSpec {
        let s = token.trim();
        if s.is_empty() {
            return ToolSpec::Other(String::new());
        }
        if s == "Skill" {
            return ToolSpec::Skill;
        }
        if s == "Agent" {
            return ToolSpec::Agent(Vec::new());
        }
        if let Some(rest) = s.strip_prefix("Agent(") {
            if let Some(inner) = rest.strip_suffix(')') {
                let names = inner
                    .split(',')
                    .map(|p| p.trim().to_string())
                    .filter(|p| !p.is_empty())
                    .collect();
                return ToolSpec::Agent(names);
            }
        }
        if let Some(rest) = s.strip_prefix("mcp__") {
            return match rest.split_once("__") {
                Some((server, tool)) => ToolSpec::Mcp {
                    server: server.to_string(),
                    tool: if tool == "*" || tool.is_empty() {
                        None
                    } else {
                        Some(tool.to_string())
                    },
                },
                None => ToolSpec::Mcp {
                    server: rest.to_string(),
                    tool: None,
                },
            };
        }
        ToolSpec::Builtin(s.to_string())
    }

    /// Render back to the canonical token form.
    pub fn to_token(&self) -> String {
        match self {
            ToolSpec::Builtin(s) | ToolSpec::Other(s) => s.clone(),
            ToolSpec::Skill => "Skill".to_string(),
            ToolSpec::Agent(names) => {
                if names.is_empty() {
                    "Agent".to_string()
                } else {
                    format!("Agent({})", names.join(", "))
                }
            }
            ToolSpec::Mcp { server, tool } => match tool {
                Some(t) => format!("mcp__{server}__{t}"),
                None => format!("mcp__{server}"),
            },
        }
    }

    /// A short category label for grouping/styling chips.
    pub fn category(&self) -> &'static str {
        match self {
            ToolSpec::Builtin(_) => "builtin",
            ToolSpec::Mcp { .. } => "mcp",
            ToolSpec::Agent(_) => "agent",
            ToolSpec::Skill => "skill",
            ToolSpec::Other(_) => "other",
        }
    }
}

/// A parsed tool list. Deserializes from either a YAML sequence or a single
/// delimited string.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolList(pub Vec<ToolSpec>);

impl ToolList {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, ToolSpec> {
        self.0.iter()
    }

    /// Render back to a comma-separated string (used when writing a single-line
    /// `tools: a, b` form).
    pub fn to_inline(&self) -> String {
        self.0
            .iter()
            .map(ToolSpec::to_token)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Split a delimited tools string into tokens, respecting parentheses so the
/// commas inside `Agent(a, b)` are not treated as separators.
pub fn split_tool_tokens(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth: i32 = 0;
    let mut cur = String::new();
    for c in s.chars() {
        match c {
            '(' => {
                depth += 1;
                cur.push(c);
            }
            ')' => {
                depth -= 1;
                cur.push(c);
            }
            ',' if depth == 0 => push_token(&mut out, &mut cur),
            c if c.is_whitespace() && depth == 0 => push_token(&mut out, &mut cur),
            c => cur.push(c),
        }
    }
    push_token(&mut out, &mut cur);
    out
}

fn push_token(out: &mut Vec<String>, cur: &mut String) {
    let t = cur.trim();
    if !t.is_empty() {
        out.push(t.to_string());
    }
    cur.clear();
}

impl<'de> Deserialize<'de> for ToolList {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ToolListVisitor;

        impl<'de> Visitor<'de> for ToolListVisitor {
            type Value = ToolList;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a tools string or a sequence of tool names")
            }

            fn visit_str<E>(self, v: &str) -> Result<ToolList, E>
            where
                E: de::Error,
            {
                Ok(ToolList(
                    split_tool_tokens(v)
                        .iter()
                        .map(|t| ToolSpec::parse(t))
                        .collect(),
                ))
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<ToolList, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut v = Vec::new();
                while let Some(item) = seq.next_element::<String>()? {
                    v.push(ToolSpec::parse(&item));
                }
                Ok(ToolList(v))
            }
        }

        deserializer.deserialize_any(ToolListVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tokens() {
        assert_eq!(ToolSpec::parse("Read"), ToolSpec::Builtin("Read".into()));
        assert_eq!(ToolSpec::parse("Skill"), ToolSpec::Skill);
        assert_eq!(
            ToolSpec::parse("mcp__github"),
            ToolSpec::Mcp {
                server: "github".into(),
                tool: None
            }
        );
        assert_eq!(
            ToolSpec::parse("mcp__github__*"),
            ToolSpec::Mcp {
                server: "github".into(),
                tool: None
            }
        );
        assert_eq!(
            ToolSpec::parse("mcp__github__list_issues"),
            ToolSpec::Mcp {
                server: "github".into(),
                tool: Some("list_issues".into())
            }
        );
        assert_eq!(
            ToolSpec::parse("Agent(worker, researcher)"),
            ToolSpec::Agent(vec!["worker".into(), "researcher".into()])
        );
        assert_eq!(ToolSpec::parse("Agent"), ToolSpec::Agent(vec![]));
    }

    #[test]
    fn paren_aware_split() {
        let toks = split_tool_tokens("Read, Agent(a, b), mcp__x__*  Bash");
        assert_eq!(toks, vec!["Read", "Agent(a, b)", "mcp__x__*", "Bash"]);
    }

    #[test]
    fn roundtrip_tokens() {
        for t in [
            "Read",
            "Skill",
            "Agent(a, b)",
            "mcp__github",
            "mcp__github__x",
        ] {
            assert_eq!(ToolSpec::parse(t).to_token(), t);
        }
    }
}
