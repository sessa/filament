//! Settings builders for the embedded terminal (`iced_term`).
//!
//! Ghostty itself can't be embedded in an Iced/wgpu app yet (its renderer isn't
//! released), so the integrated terminal is `iced_term`, backed by
//! `alacritty_terminal` + `portable-pty`. These helpers produce the backend
//! settings for either a plain shell or a Claude Code session.

use std::path::PathBuf;

use iced_term::settings::{BackendSettings, Settings};

/// A plain interactive shell (the user's `$SHELL`) in `cwd`.
pub fn shell_settings(cwd: Option<PathBuf>) -> Settings {
    let program = std::env::var("SHELL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(default_shell);
    backend(program, Vec::new(), cwd)
}

/// An interactive Claude Code session in the workspace directory.
pub fn agent_settings(cwd: Option<PathBuf>) -> Settings {
    backend("claude".to_string(), Vec::new(), cwd)
}

fn backend(program: String, args: Vec<String>, cwd: Option<PathBuf>) -> Settings {
    Settings {
        backend: BackendSettings {
            program,
            args,
            working_directory: cwd,
            ..Default::default()
        },
        ..Default::default()
    }
}

fn default_shell() -> String {
    if cfg!(windows) {
        "powershell.exe".to_string()
    } else {
        "/bin/bash".to_string()
    }
}
