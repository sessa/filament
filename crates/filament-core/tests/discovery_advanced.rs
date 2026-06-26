//! Discovery layouts the single static fixture can't express: the managed and
//! plugin scopes, `settings.local.json`, the git-root boundary, nearest-project
//! precedence, and the `include_user` switch.

mod common;

use common::Sandbox;
use filament_core::{DiscoveryOptions, ItemKind, Scope, Workspace};

/// Find the single agent named `name`, asserting it exists.
fn agents<'a>(ws: &'a Workspace, name: &str) -> Vec<&'a filament_core::Entry> {
    ws.catalog
        .by_kind(ItemKind::Agent)
        .filter(|e| e.name == name)
        .collect()
}

#[test]
fn managed_scope_is_discovered_and_wins() {
    let sb = Sandbox::new();
    // Managed dir *is* the claude dir (it holds `agents/` directly).
    sb.agent("managed/agents/dup.md", "dup");
    sb.agent("project/.claude/agents/dup.md", "dup");

    let opts = DiscoveryOptions {
        workspace: Some(sb.join("project")),
        home: None,
        managed: Some(sb.join("managed")),
        include_user: false,
    };
    let ws = Workspace::load(opts);

    let dups = agents(&ws, "dup");
    assert_eq!(dups.len(), 2, "one managed + one project");
    let winner = dups.iter().find(|e| e.winning).expect("a winner");
    assert_eq!(
        winner.scope,
        Scope::Managed,
        "managed has highest precedence"
    );
}

#[test]
fn managed_path_that_is_not_a_directory_is_ignored() {
    let sb = Sandbox::new();
    sb.agent("project/.claude/agents/solo.md", "solo");

    let opts = DiscoveryOptions {
        workspace: Some(sb.join("project")),
        home: None,
        managed: Some(sb.join("does-not-exist")),
        include_user: false,
    };
    let ws = Workspace::load(opts);

    let solo = agents(&ws, "solo");
    assert_eq!(solo.len(), 1);
    assert_eq!(solo[0].scope, Scope::Project);
}

#[test]
fn plugins_are_discovered_at_plugin_scope_and_lose_to_user() {
    let sb = Sandbox::new();
    sb.agent("home/.claude/agents/dup.md", "dup");
    sb.agent("home/.claude/plugins/acme/agents/dup.md", "dup");
    sb.agent(
        "home/.claude/plugins/acme/agents/plugin-only.md",
        "plugin-only",
    );

    let opts = DiscoveryOptions {
        workspace: None,
        home: Some(sb.join("home")),
        managed: None,
        include_user: true,
    };
    let ws = Workspace::load(opts);

    // The plugin-only agent is present and tagged Plugin.
    let only = agents(&ws, "plugin-only");
    assert_eq!(only.len(), 1);
    assert_eq!(only[0].scope, Scope::Plugin);

    // On a name clash, the user definition wins over the plugin one.
    let dups = agents(&ws, "dup");
    assert_eq!(dups.len(), 2);
    let winner = dups.iter().find(|e| e.winning).expect("a winner");
    let loser = dups.iter().find(|e| !e.winning).expect("a shadowed one");
    assert_eq!(winner.scope, Scope::User);
    assert_eq!(loser.scope, Scope::Plugin);
    assert_eq!(loser.shadowed_by.as_ref(), Some(&winner.id));
}

#[test]
fn settings_local_is_discovered_and_layers_rather_than_shadows() {
    let sb = Sandbox::new();
    sb.write(
        "project/.claude/settings.json",
        "{\"env\": {\"A\": \"1\"}}\n",
    );
    sb.write(
        "project/.claude/settings.local.json",
        "{\"env\": {\"B\": \"2\"}}\n",
    );

    let opts = DiscoveryOptions {
        workspace: Some(sb.join("project")),
        home: None,
        managed: None,
        include_user: false,
    };
    let ws = Workspace::load(opts);

    let settings: Vec<_> = ws.catalog.by_kind(ItemKind::Settings).collect();
    assert_eq!(settings.len(), 2, "settings.json + settings.local.json");
    // Settings layer rather than resolve by name, so neither shadows the other.
    assert!(
        settings.iter().all(|e| e.winning),
        "both settings entries should be winning"
    );
}

#[test]
fn discovery_stops_at_the_git_root() {
    let sb = Sandbox::new();
    // A `.claude` *above* the repo must never be reached.
    sb.agent("outside/.claude/agents/outsider.md", "outsider");
    sb.dir("outside/repo/.git");
    sb.agent("outside/repo/.claude/agents/dup.md", "dup");
    sb.agent("outside/repo/sub/.claude/agents/dup.md", "dup");

    let opts = DiscoveryOptions {
        workspace: Some(sb.join("outside/repo/sub")),
        home: None,
        managed: None,
        include_user: false,
    };
    let ws = Workspace::load(opts);

    assert!(
        agents(&ws, "outsider").is_empty(),
        "config above the git root must not be discovered"
    );

    // Nearest `.claude` (in `sub/`) wins over the repo-root one.
    let dups = agents(&ws, "dup");
    assert_eq!(dups.len(), 2);
    let winner = dups.iter().find(|e| e.winning).expect("a winner");
    assert!(
        winner.source_path.to_string_lossy().contains("sub"),
        "nearest project .claude should win, got {}",
        winner.source_path.display()
    );
}

#[test]
fn include_user_false_excludes_user_and_plugins() {
    let sb = Sandbox::new();
    sb.agent("home/.claude/agents/user-only.md", "user-only");
    sb.agent("home/.claude/plugins/acme/agents/plug.md", "plug");
    sb.agent("project/.claude/agents/proj-only.md", "proj-only");

    let opts = DiscoveryOptions {
        workspace: Some(sb.join("project")),
        home: Some(sb.join("home")),
        managed: None,
        include_user: false,
    };
    let ws = Workspace::load(opts);

    assert_eq!(agents(&ws, "proj-only").len(), 1);
    assert!(agents(&ws, "user-only").is_empty());
    assert!(agents(&ws, "plug").is_empty());
    assert!(
        ws.catalog.entries.iter().all(|e| e.scope == Scope::Project),
        "only project-scoped entries should be present"
    );
}

#[test]
fn rescan_picks_up_a_newly_added_file() {
    let sb = Sandbox::new();
    sb.agent("project/.claude/agents/first.md", "first");

    let opts = DiscoveryOptions {
        workspace: Some(sb.join("project")),
        home: None,
        managed: None,
        include_user: false,
    };
    let mut ws = Workspace::load(opts);
    assert_eq!(ws.catalog.by_kind(ItemKind::Agent).count(), 1);

    sb.agent("project/.claude/agents/second.md", "second");
    ws.rescan();
    assert_eq!(
        ws.catalog.by_kind(ItemKind::Agent).count(),
        2,
        "rescan should observe the new file"
    );
}
