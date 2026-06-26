mod common;

use common::fixtures;
use filament_core::parse::parse_agent;
use filament_core::{AgentColor, Effort, Memory, ModelChoice, Payload, Scope, ToolSpec};

#[test]
fn parses_full_agent_with_inline_tools() {
    let path = fixtures().join("workspace_a/.claude/agents/code-reviewer.md");
    let entry = parse_agent(&path, Scope::Project, 0);

    let Payload::Agent(fm) = &entry.payload else {
        panic!("expected an agent payload");
    };
    assert_eq!(fm.name, "code-reviewer");
    assert_eq!(fm.model, ModelChoice::Sonnet);
    assert_eq!(fm.color, Some(AgentColor::Green));
    assert_eq!(fm.effort, Some(Effort::High));

    let tools = fm.tools.as_ref().expect("tools present");
    assert!(tools.0.contains(&ToolSpec::Builtin("Read".into())));
    assert!(tools
        .0
        .iter()
        .any(|t| matches!(t, ToolSpec::Agent(v) if v == &vec!["debugger".to_string()])));
    assert!(tools.0.iter().any(
        |t| matches!(t, ToolSpec::Mcp { server, tool } if server == "github" && tool.is_none())
    ));

    let disallowed = fm.disallowed_tools.as_ref().expect("disallowed present");
    assert_eq!(disallowed.0, vec![ToolSpec::Builtin("Write".into())]);

    assert!(fm.when_to_use.is_some());
    assert!(fm.effective_description().contains("Invoke immediately"));
    assert!(entry.body().unwrap().contains("senior code reviewer"));
}

#[test]
fn parses_yaml_list_tools() {
    let path = fixtures().join("workspace_a/.claude/agents/debugger.md");
    let entry = parse_agent(&path, Scope::Project, 0);
    let Payload::Agent(fm) = &entry.payload else {
        panic!("expected an agent payload");
    };
    let tools = fm.tools.as_ref().unwrap();
    assert_eq!(tools.0.len(), 4);
    assert_eq!(fm.memory, Some(Memory::Project));
}

#[test]
fn accepts_alternate_field_spellings() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("x.md");
    std::fs::write(
        &path,
        "---\nname: x\ndescription: d\nwhenToUse: later\ndisallowed-tools: Write\nmaxTurns: 5\ninitial-prompt: go\n---\nbody\n",
    )
    .unwrap();

    let entry = parse_agent(&path, Scope::User, 0);
    let Payload::Agent(fm) = &entry.payload else {
        panic!("expected an agent payload");
    };
    assert_eq!(fm.when_to_use.as_deref(), Some("later"));
    assert_eq!(fm.max_turns, Some(5));
    assert_eq!(fm.initial_prompt.as_deref(), Some("go"));
    assert!(fm.disallowed_tools.is_some());
}

#[test]
fn malformed_frontmatter_yields_invalid_entry() {
    let path = fixtures().join("workspace_a/.claude/agents/broken.md");
    let entry = parse_agent(&path, Scope::Project, 0);
    assert!(!entry.is_valid());
    // name falls back to the filename so it can still be listed.
    assert_eq!(entry.name, "broken");
}
