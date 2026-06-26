//! `Catalog::resolve` collision handling beyond the simple two-entry case:
//! three-way shadowing, the precedence tie-break, settings not shadowing, and
//! invalid entries being excluded from resolution.

mod common;

use common::Sandbox;
use filament_core::{DiscoveryOptions, ItemKind, Scope, Workspace};

fn load(sb: &Sandbox) -> Workspace {
    Workspace::load(DiscoveryOptions {
        workspace: Some(sb.join("project")),
        home: Some(sb.join("home")),
        managed: Some(sb.join("managed")),
        include_user: true,
    })
}

#[test]
fn three_way_collision_has_one_winner_and_two_shadows() {
    let sb = Sandbox::new();
    sb.agent("managed/agents/tri.md", "tri");
    sb.agent("project/.claude/agents/tri.md", "tri");
    sb.agent("home/.claude/agents/tri.md", "tri");

    let ws = load(&sb);
    let tris: Vec<_> = ws
        .catalog
        .by_kind(ItemKind::Agent)
        .filter(|e| e.name == "tri")
        .collect();
    assert_eq!(tris.len(), 3);

    let winners: Vec<_> = tris.iter().filter(|e| e.winning).collect();
    assert_eq!(winners.len(), 1, "exactly one winner");
    let winner = winners[0];
    assert_eq!(winner.scope, Scope::Managed);
    assert_eq!(winner.shadows.len(), 2, "winner records both shadowed ids");

    // Every loser points back at the winner.
    for loser in tris.iter().filter(|e| !e.winning) {
        assert_eq!(loser.shadowed_by.as_ref(), Some(&winner.id));
        assert!(winner.shadows.contains(&loser.id));
    }
}

#[test]
fn precedence_tie_is_broken_by_source_path() {
    // Two agents with the same name in the *same* root share a precedence, so
    // resolution falls back to the lexicographically smaller path.
    let sb = Sandbox::new();
    sb.agent("project/.claude/agents/a.md", "twin");
    sb.agent("project/.claude/agents/b.md", "twin");

    let ws = Workspace::load(DiscoveryOptions {
        workspace: Some(sb.join("project")),
        home: None,
        managed: None,
        include_user: false,
    });

    let twins: Vec<_> = ws
        .catalog
        .by_kind(ItemKind::Agent)
        .filter(|e| e.name == "twin")
        .collect();
    assert_eq!(twins.len(), 2);
    let winner = twins.iter().find(|e| e.winning).expect("a winner");
    assert!(
        winner.source_path.ends_with("a.md"),
        "the lexicographically-smaller path should win, got {}",
        winner.source_path.display()
    );
}

#[test]
fn settings_never_shadow_each_other() {
    let sb = Sandbox::new();
    sb.write("project/.claude/settings.json", "{}\n");
    sb.write("home/.claude/settings.json", "{}\n");

    let ws = Workspace::load(DiscoveryOptions {
        workspace: Some(sb.join("project")),
        home: Some(sb.join("home")),
        managed: None,
        include_user: true,
    });

    let settings: Vec<_> = ws.catalog.by_kind(ItemKind::Settings).collect();
    assert_eq!(settings.len(), 2);
    assert!(settings
        .iter()
        .all(|e| e.winning && e.shadowed_by.is_none()));
}

#[test]
fn an_invalid_entry_does_not_shadow_or_get_shadowed() {
    let sb = Sandbox::new();
    // Valid project agent named `clash`.
    sb.agent("project/.claude/agents/clash.md", "clash");
    // A malformed user agent that also reports the name `clash` via its filename
    // fallback (its frontmatter is broken).
    sb.write(
        "home/.claude/agents/clash.md",
        "---\nname: \"unterminated\ncolor: bogus\n---\nbody\n",
    );

    let ws = Workspace::load(DiscoveryOptions {
        workspace: Some(sb.join("project")),
        home: Some(sb.join("home")),
        managed: None,
        include_user: true,
    });

    let valid = ws
        .catalog
        .by_kind(ItemKind::Agent)
        .find(|e| e.name == "clash" && e.is_valid())
        .expect("valid clash present");
    let invalid = ws
        .catalog
        .by_kind(ItemKind::Agent)
        .find(|e| !e.is_valid())
        .expect("invalid entry present");

    // Invalid entries are excluded from name resolution: both stay winning.
    assert!(valid.winning && valid.shadowed_by.is_none());
    assert!(invalid.winning && invalid.shadowed_by.is_none());
    assert_eq!(ws.catalog.invalid_count(), 1);
}
