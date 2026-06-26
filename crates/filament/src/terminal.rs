//! Settings builders for the embedded terminal (`iced_term`).
//!
//! Ghostty itself can't be embedded in an Iced/wgpu app yet (its renderer isn't
//! released), so the integrated terminal is `iced_term`, backed by
//! `alacritty_terminal` + `portable-pty`. These helpers produce the backend
//! settings for either a plain shell or a Claude Code session, themed to match
//! the app (Tokyo Night palette, JetBrains Mono).

use std::path::PathBuf;

use iced::Font;
use iced_term::settings::{BackendSettings, FontSettings, Settings, ThemeSettings};
use iced_term::ColorPalette;

/// A plain interactive shell (the user's `$SHELL`) in `cwd`.
pub fn shell_settings(cwd: Option<PathBuf>) -> Settings {
    settings(default_shell(), Vec::new(), cwd)
}

/// An interactive Claude Code session in the workspace directory.
pub fn agent_settings(cwd: Option<PathBuf>) -> Settings {
    settings("claude".to_string(), Vec::new(), cwd)
}

fn settings(program: String, args: Vec<String>, cwd: Option<PathBuf>) -> Settings {
    Settings {
        font: FontSettings {
            size: 13.0,
            font_type: Font::with_name("JetBrains Mono"),
            ..Default::default()
        },
        theme: ThemeSettings::new(Box::new(tokyo_night())),
        backend: BackendSettings {
            program,
            args,
            working_directory: cwd,
            ..Default::default()
        },
    }
}

fn default_shell() -> String {
    if cfg!(windows) {
        "powershell.exe".to_string()
    } else {
        std::env::var("SHELL")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "/bin/bash".to_string())
    }
}

/// Tokyo Night palette, to match the app's default dark theme.
fn tokyo_night() -> ColorPalette {
    let s = |h: &str| h.to_string();
    ColorPalette {
        foreground: s("#c0caf5"),
        background: s("#1a1b26"),
        black: s("#15161e"),
        red: s("#f7768e"),
        green: s("#9ece6a"),
        yellow: s("#e0af68"),
        blue: s("#7aa2f7"),
        magenta: s("#bb9af7"),
        cyan: s("#7dcfff"),
        white: s("#a9b1d6"),
        bright_black: s("#414868"),
        bright_red: s("#f7768e"),
        bright_green: s("#9ece6a"),
        bright_yellow: s("#e0af68"),
        bright_blue: s("#7aa2f7"),
        bright_magenta: s("#bb9af7"),
        bright_cyan: s("#7dcfff"),
        bright_white: s("#c0caf5"),
        bright_foreground: Some(s("#c0caf5")),
        dim_foreground: s("#828bb8"),
        dim_black: s("#15161e"),
        dim_red: s("#c0556a"),
        dim_green: s("#7a9c50"),
        dim_yellow: s("#a9844f"),
        dim_blue: s("#5d7bbb"),
        dim_magenta: s("#8d75bb"),
        dim_cyan: s("#5e9cbf"),
        dim_white: s("#828bb8"),
    }
}
