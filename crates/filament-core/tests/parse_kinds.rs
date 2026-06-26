//! Direct tests for the non-agent parsers (`parse_skill`, `parse_command`,
//! `parse_settings`, `parse_mcp_file`, `parse_inline_mcp`) and the "a malformed
//! file is a diagnostic, never a panic" guarantee for every kind.

mod common;

use common::{fixtures, Sandbox};
use filament_core::parse::{
    parse_command, parse_inline_mcp, parse_mcp_file, parse_settings, parse_skill,
};
use filament_core::{McpTransport, ParseError, Payload, Scope};

// ---- skills -----------------------------------------------------------------

#[test]
fn parses_a_valid_skill() {
    let path = fixtures().join("workspace_a/.claude/skills/deploy/SKILL.md");
    let entry = parse_skill(&path, Scope::Project, 0);
    let Payload::Skill(fm) = &entry.payload else {
        panic!("expected a skill payload");
    };
    assert_eq!(fm.name, "deploy");
    assert!(fm.allowed_tools.is_some());
    assert_eq!(fm.argument_hint.as_deref(), Some("[environment]"));
}

#[test]
fn parses_skill_alternate_spellings() {
    let path = fixtures().join("workspace_a/.claude/skills/format-code/SKILL.md");
    let entry = parse_skill(&path, Scope::Project, 0);
    let Payload::Skill(fm) = &entry.payload else {
        panic!("expected a skill payload");
    };
    assert_eq!(fm.disable_model_invocation, Some(true));
}

#[test]
fn skill_without_frontmatter_is_invalid_with_dir_name_fallback() {
    let sb = Sandbox::new();
    let path = sb.write("skills/lonely/SKILL.md", "Just a body, no frontmatter.\n");
    let entry = parse_skill(&path, Scope::User, 0);
    assert!(!entry.is_valid());
    assert!(matches!(
        entry.payload.error(),
        Some(ParseError::MissingFrontmatter { .. })
    ));
    // The name falls back to the containing directory so it can still be listed.
    assert_eq!(entry.name, "lonely");
}

#[test]
fn skill_with_broken_yaml_falls_back_to_dir_name() {
    let sb = Sandbox::new();
    let path = sb.write(
        "skills/broken-skill/SKILL.md",
        "---\nname: \"unterminated\n---\nbody\n",
    );
    let entry = parse_skill(&path, Scope::User, 0);
    assert!(!entry.is_valid());
    assert!(matches!(
        entry.payload.error(),
        Some(ParseError::Yaml { .. })
    ));
    assert_eq!(entry.name, "broken-skill");
}

// ---- commands ---------------------------------------------------------------

#[test]
fn parses_command_with_frontmatter() {
    let path = fixtures().join("workspace_a/.claude/commands/release.md");
    let entry = parse_command(&path, Scope::Project, 0);
    let Payload::Command(fm) = &entry.payload else {
        panic!("expected a command payload");
    };
    assert_eq!(fm.description.as_deref(), Some("Cut a new release."));
    assert_eq!(fm.argument_hint.as_deref(), Some("[version]"));
    // No `name` field, so the display name comes from the filename.
    assert_eq!(entry.name, "release");
}

#[test]
fn command_without_frontmatter_is_valid_default() {
    let path = fixtures().join("workspace_a/.claude/commands/note.md");
    let entry = parse_command(&path, Scope::Project, 0);
    assert!(entry.is_valid());
    let Payload::Command(fm) = &entry.payload else {
        panic!("expected a command payload");
    };
    assert!(fm.name.is_none());
    assert_eq!(entry.name, "note");
    assert!(entry.body().unwrap().contains("Capture a quick note"));
}

#[test]
fn command_with_broken_frontmatter_is_invalid() {
    let sb = Sandbox::new();
    let path = sb.write("commands/x.md", "---\nmodel: [unterminated\n---\nbody\n");
    let entry = parse_command(&path, Scope::Project, 0);
    assert!(!entry.is_valid());
    assert_eq!(entry.name, "x");
}

// ---- settings ---------------------------------------------------------------

#[test]
fn parses_settings_with_permissions_hooks_and_env() {
    let path = fixtures().join("workspace_a/.claude/settings.json");
    let entry = parse_settings(&path, Scope::Project, 0);
    let Payload::Settings(s) = &entry.payload else {
        panic!("expected a settings payload");
    };
    assert!(s.permissions.allow.contains(&"Skill".to_string()));
    assert!(s.permissions.deny.iter().any(|d| d.contains("rm -rf")));
    assert_eq!(s.env.get("FILAMENT_ENV").map(String::as_str), Some("test"));

    let groups = s.hook_groups();
    assert!(groups.iter().any(|g| g.event == "PreToolUse"));
    assert!(groups.iter().any(|g| g.event == "Stop"));
    // `$schema` is an unmodeled key; it should be preserved in `extra`.
    assert!(s.extra.contains_key("$schema"));
}

#[test]
fn invalid_settings_json_is_a_diagnostic() {
    let sb = Sandbox::new();
    let path = sb.write("settings.json", "{ not valid json ,, }");
    let entry = parse_settings(&path, Scope::Project, 0);
    assert!(!entry.is_valid());
    assert!(matches!(
        entry.payload.error(),
        Some(ParseError::Json { .. })
    ));
}

#[test]
fn unreadable_settings_is_a_diagnostic() {
    let sb = Sandbox::new();
    let missing = sb.join("nope/settings.json");
    let entry = parse_settings(&missing, Scope::Project, 0);
    assert!(!entry.is_valid());
    assert!(matches!(
        entry.payload.error(),
        Some(ParseError::Unreadable { .. })
    ));
}

// ---- MCP servers ------------------------------------------------------------

#[test]
fn parses_mcp_file_into_one_entry_per_server() {
    let path = fixtures().join("workspace_a/.mcp.json");
    let entries = parse_mcp_file(&path, Scope::Project, 0);
    assert_eq!(entries.len(), 2);

    let github = entries.iter().find(|e| e.name == "github").unwrap();
    let Payload::Mcp(server) = &github.payload else {
        panic!("expected an mcp payload");
    };
    assert!(matches!(server.transport, McpTransport::Http { .. }));

    let pw = entries.iter().find(|e| e.name == "playwright").unwrap();
    let Payload::Mcp(server) = &pw.payload else {
        panic!("expected an mcp payload");
    };
    assert_eq!(server.transport.kind(), "stdio");
}

#[test]
fn mcp_file_without_servers_key_yields_no_entries() {
    let sb = Sandbox::new();
    let path = sb.write(".mcp.json", "{\"somethingElse\": {}}\n");
    let entries = parse_mcp_file(&path, Scope::Project, 0);
    assert!(entries.is_empty());
}

#[test]
fn invalid_mcp_json_yields_a_single_diagnostic() {
    let sb = Sandbox::new();
    let path = sb.write(".mcp.json", "{ broken");
    let entries = parse_mcp_file(&path, Scope::Project, 0);
    assert_eq!(entries.len(), 1);
    assert!(!entries[0].is_valid());
    assert!(matches!(
        entries[0].payload.error(),
        Some(ParseError::Json { .. })
    ));
}

#[test]
fn unreadable_mcp_file_yields_a_single_diagnostic() {
    let sb = Sandbox::new();
    let entries = parse_mcp_file(&sb.join("nope/.mcp.json"), Scope::Project, 0);
    assert_eq!(entries.len(), 1);
    assert!(matches!(
        entries[0].payload.error(),
        Some(ParseError::Unreadable { .. })
    ));
}

#[test]
fn a_bad_server_is_isolated_to_its_own_entry() {
    let sb = Sandbox::new();
    let path = sb.write(
        ".mcp.json",
        r#"{ "mcpServers": {
            "good": { "command": "npx" },
            "bad":  { "type": "stdio" }
        } }"#,
    );
    let entries = parse_mcp_file(&path, Scope::Project, 0);
    assert_eq!(entries.len(), 2);
    let good = entries.iter().find(|e| e.name == "good").unwrap();
    let bad = entries.iter().find(|e| e.name == "bad").unwrap();
    assert!(
        good.is_valid(),
        "a valid server is unaffected by a bad sibling"
    );
    assert!(!bad.is_valid());
    assert!(matches!(
        bad.payload.error(),
        Some(ParseError::Other { .. })
    ));
}

#[test]
fn parses_inline_mcp_from_yaml() {
    let yaml = "\
local-stdio:
  command: my-server
  args: [--flag]
remote:
  url: https://example.com/mcp
";
    let value: serde_norway::Value = serde_norway::from_str(yaml).unwrap();
    let servers = parse_inline_mcp(&value);
    assert_eq!(servers.len(), 2);
    assert!(servers.iter().any(|s| s.transport.kind() == "stdio"));
    assert!(servers.iter().any(|s| s.transport.kind() == "http"));
}

#[test]
fn inline_mcp_from_non_mapping_is_empty() {
    let value: serde_norway::Value = serde_norway::from_str("- just\n- a\n- list\n").unwrap();
    assert!(parse_inline_mcp(&value).is_empty());
}
