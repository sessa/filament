mod common;

use common::fixtures;
use filament_core::edit::set_scalar;
use filament_core::frontmatter::split_frontmatter;
use filament_core::parse::parse_agent;
use filament_core::{ModelChoice, Payload, Scope};

/// Changing one scalar must leave every other byte of the file intact.
#[test]
fn changing_model_is_lossless() {
    let path = fixtures().join("workspace_a/.claude/agents/code-reviewer.md");
    let original = std::fs::read_to_string(&path).unwrap();

    let split = split_frontmatter(&original);
    let edited = set_scalar(&original, split.frontmatter.clone(), "model", "opus");

    // The model line changed; nothing else did.
    assert!(edited.contains("model: opus"));
    assert!(!edited.contains("model: sonnet"));
    assert!(edited.contains("name: code-reviewer"));
    assert!(edited.contains("color: green"));
    assert!(edited.contains("senior code reviewer"));

    // The only line removed from the original is the old model line.
    let dropped: Vec<&str> = original
        .lines()
        .filter(|l| !edited.lines().any(|e| e == *l))
        .collect();
    assert_eq!(dropped, vec!["model: sonnet"]);

    // And the edited text reparses with the new model.
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("code-reviewer.md");
    std::fs::write(&p, &edited).unwrap();
    let entry = parse_agent(&p, Scope::Project, 0);
    let Payload::Agent(fm) = &entry.payload else {
        panic!("expected an agent payload");
    };
    assert_eq!(fm.model, ModelChoice::Opus);
}

/// Appending a previously-absent key must keep the existing keys and body.
#[test]
fn appending_a_key_preserves_everything_else() {
    let path = fixtures().join("home/.claude/agents/researcher.md");
    let original = std::fs::read_to_string(&path).unwrap();
    let split = split_frontmatter(&original);

    let edited = set_scalar(&original, split.frontmatter.clone(), "background", "true");
    assert!(edited.contains("background: true"));
    assert!(edited.contains("name: researcher"));
    assert!(edited.contains("You are a thorough researcher"));
}
