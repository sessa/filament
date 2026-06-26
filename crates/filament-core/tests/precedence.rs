mod common;

use common::load_workspace_a;
use filament_core::{ItemKind, Scope};

#[test]
fn project_agent_shadows_user_agent_of_same_name() {
    let ws = load_workspace_a();
    let reviewers: Vec<_> = ws
        .catalog
        .by_kind(ItemKind::Agent)
        .filter(|e| e.name == "code-reviewer")
        .collect();
    assert_eq!(reviewers.len(), 2, "one project + one user code-reviewer");

    let winner = reviewers.iter().find(|e| e.winning).expect("a winner");
    let loser = reviewers
        .iter()
        .find(|e| !e.winning)
        .expect("a shadowed one");

    assert_eq!(winner.scope, Scope::Project);
    assert_eq!(loser.scope, Scope::User);
    assert_eq!(loser.shadowed_by.as_ref(), Some(&winner.id));
    assert!(winner.shadows.contains(&loser.id));
}

#[test]
fn unique_names_are_all_winning() {
    let ws = load_workspace_a();
    let researcher = ws
        .catalog
        .by_kind(ItemKind::Agent)
        .find(|e| e.name == "researcher")
        .unwrap();
    assert!(researcher.winning);
    assert!(researcher.shadowed_by.is_none());
}
