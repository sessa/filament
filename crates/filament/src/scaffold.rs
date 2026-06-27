//! First-run scaffolding — the Claude Code skill that drives Filament sessions.
//!
//! crow ships a `/crow-workspace` skill so the manager Claude can spin up
//! worktree sessions from an issue URL or a plain-English task. Filament's
//! equivalent is a `filament-workspace` skill that calls the `filament` CLI
//! (which talks to the running app over the IPC socket). [`write_workspace_skill`]
//! writes it into a project's `.claude/skills/` directory.

use std::io;
use std::path::{Path, PathBuf};

/// The skill body — instructions + the CLI surface the manager should use.
const WORKSPACE_SKILL: &str = r#"---
name: filament-workspace
description: Spin up an isolated git worktree session (with Claude Code) for a GitHub/GitLab issue or a free-text task, using the Filament CLI. Use when asked to "start working on", "open a workspace for", or "create a session for" an issue or task.
---

# filament-workspace

You orchestrate Filament work **sessions**. A session pairs a git worktree (an
isolated checkout on its own branch) with a Claude Code instance and, optionally,
a linked issue / pull request. Drive the running Filament app through its CLI,
which speaks to it over a local socket.

## When invoked

Given an issue URL/number or a natural-language task description:

1. Create the session in the Filament UI (or, if a session already exists, find
   it with `filament list-sessions`).
2. Attach the ticket and any reference links.
3. Open a terminal in the worktree and start work.

## CLI reference

All commands print JSON on stdout.

- `filament list-sessions` — every session.
- `filament get-session --session <id>` — one session's full details.
- `filament set-status --session <id> <active|paused|inReview|completed|archived>`
- `filament rename-session --session <id> <name>`
- `filament set-ticket --session <id> [--url <url>] [--title <text>] [--number <n>]`
- `filament add-link --session <id> --label <text> --url <url> [--type <kind>]`
- `filament list-links --session <id>`
- `filament new-terminal --session <id> --cwd <path> [--name <text>] [--command <text>]`
- `filament list-terminals --session <id>`
- `filament rename-terminal --session <id> --terminal <id> <name>`
- `filament close-terminal --session <id> --terminal <id>`
- `filament send --session <id> [--terminal <id>] <text>` — type into a terminal.
- `filament select-session --session <id>` — focus it in the UI.

## Notes

- New sessions that need a fresh worktree are created from the Filament UI
  ("New session"); from here, prefer attaching to an existing session and
  driving its terminals.
- Keep the session's ticket and links up to date so the board reflects reality.
"#;

/// Write the `filament-workspace` skill into `<base>/.claude/skills/filament-workspace/SKILL.md`.
/// Returns the path written. Overwrites an existing copy so updates land.
pub fn write_workspace_skill(base: &Path) -> io::Result<PathBuf> {
    let dir = base
        .join(".claude")
        .join("skills")
        .join("filament-workspace");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("SKILL.md");
    std::fs::write(&path, WORKSPACE_SKILL)?;
    Ok(path)
}
