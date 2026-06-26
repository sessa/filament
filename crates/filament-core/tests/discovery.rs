mod common;

use common::load_workspace_a;
use filament_core::ItemKind;

#[test]
fn finds_all_kinds() {
    let ws = load_workspace_a();
    let cat = &ws.catalog;
    let count = |k| cat.by_kind(k).count();

    // project: code-reviewer, debugger, broken; home: code-reviewer, researcher
    assert_eq!(count(ItemKind::Agent), 5, "agents");
    assert_eq!(count(ItemKind::Skill), 2, "skills");
    assert_eq!(count(ItemKind::Command), 2, "commands");
    assert_eq!(count(ItemKind::McpServer), 2, "mcp servers");
    assert_eq!(count(ItemKind::Settings), 1, "settings");
}

#[test]
fn invalid_file_is_a_diagnostic_not_a_panic() {
    let ws = load_workspace_a();
    assert_eq!(ws.catalog.invalid_count(), 1);
    let broken = ws
        .catalog
        .by_kind(ItemKind::Agent)
        .find(|e| e.name == "broken")
        .expect("broken agent present");
    assert!(!broken.is_valid());
    assert!(broken.payload.error().is_some());
}

#[test]
fn command_without_frontmatter_is_valid() {
    let ws = load_workspace_a();
    let note = ws
        .catalog
        .by_kind(ItemKind::Command)
        .find(|e| e.name == "note")
        .expect("note command present");
    assert!(note.is_valid());
    assert!(note.body().unwrap().contains("Capture a quick note"));
}

#[test]
fn mcp_servers_are_split_per_server() {
    let ws = load_workspace_a();
    let names: Vec<_> = ws
        .catalog
        .by_kind(ItemKind::McpServer)
        .map(|e| e.name.clone())
        .collect();
    assert!(names.contains(&"github".to_string()));
    assert!(names.contains(&"playwright".to_string()));
}
