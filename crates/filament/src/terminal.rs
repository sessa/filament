//! Settings builders for the embedded terminal (`iced_term`).
//!
//! Ghostty itself can't be embedded in an Iced/wgpu app yet (its renderer isn't
//! released), so the integrated terminal is `iced_term`, backed by
//! `alacritty_terminal` + `portable-pty`. These helpers produce the backend
//! settings for either a plain shell or a Claude Code session, themed to match
//! the app (warm palette, JetBrains Mono) and honoring the user's font size.

use std::path::PathBuf;

use iced::Font;
use iced_term::settings::{BackendSettings, FontSettings, Settings, ThemeSettings};
use iced_term::ColorPalette;

/// How the terminal should be launched.
#[derive(Debug, Clone, Copy)]
pub struct TermOpts {
    pub dark: bool,
    pub font_size: f32,
}

/// A plain interactive shell in `cwd`. `shell` overrides `$SHELL` when non-empty.
pub fn shell_settings(cwd: Option<PathBuf>, shell: &str, opts: TermOpts) -> Settings {
    let program = if shell.trim().is_empty() {
        default_shell()
    } else {
        shell.trim().to_string()
    };
    settings(program, Vec::new(), cwd, opts)
}

/// An interactive Claude Code session in `cwd`.
pub fn agent_settings(cwd: Option<PathBuf>, opts: TermOpts) -> Settings {
    settings("claude".to_string(), Vec::new(), cwd, opts)
}

/// A Claude Code session in `cwd` with explicit extra CLI arguments.
pub fn claude_settings(cwd: Option<PathBuf>, opts: TermOpts, args: Vec<String>) -> Settings {
    settings("claude".to_string(), args, cwd, opts)
}

/// The persistent **manager** Claude session — crow's orchestration terminal.
/// Launches with `--permission-mode auto` for approval-free execution (or `plan`
/// when auto-permission is off), and `--rc` when remote control is enabled.
pub fn manager_settings(
    cwd: Option<PathBuf>,
    opts: TermOpts,
    auto_permission: bool,
    remote_control: bool,
) -> Settings {
    let mut args = vec![
        "--permission-mode".to_string(),
        if auto_permission { "auto" } else { "plan" }.to_string(),
    ];
    if remote_control {
        args.push("--rc".to_string());
    }
    claude_settings(cwd, opts, args)
}

/// Run an explicit command line (split on spaces) in `cwd`.
pub fn command_settings(cwd: Option<PathBuf>, opts: TermOpts, command: &str) -> Settings {
    let mut parts = command.split_whitespace().map(|s| s.to_string());
    let program = parts.next().unwrap_or_else(default_shell);
    settings(program, parts.collect(), cwd, opts)
}

fn settings(program: String, args: Vec<String>, cwd: Option<PathBuf>, opts: TermOpts) -> Settings {
    Settings {
        font: FontSettings {
            size: opts.font_size,
            font_type: Font::with_name("JetBrains Mono"),
            ..Default::default()
        },
        theme: ThemeSettings::new(Box::new(palette(opts.dark))),
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

/// A warm terminal palette to match the app, in dark or light.
fn palette(dark: bool) -> ColorPalette {
    let s = |h: &str| h.to_string();
    if dark {
        ColorPalette {
            foreground: s("#ECEAE3"),
            background: s("#1B1A18"),
            black: s("#1B1A18"),
            red: s("#E5675A"),
            green: s("#7FB069"),
            yellow: s("#E0AF68"),
            blue: s("#6F9BE0"),
            magenta: s("#B18AE0"),
            cyan: s("#5FB0B0"),
            white: s("#C8C4B8"),
            bright_black: s("#5A564E"),
            bright_red: s("#E5675A"),
            bright_green: s("#8FBE78"),
            bright_yellow: s("#E8BE7A"),
            bright_blue: s("#84ACED"),
            bright_magenta: s("#C29BEA"),
            bright_cyan: s("#74C2C2"),
            bright_white: s("#ECEAE3"),
            bright_foreground: Some(s("#ECEAE3")),
            dim_foreground: s("#9A958A"),
            dim_black: s("#1B1A18"),
            dim_red: s("#B5524A"),
            dim_green: s("#658C54"),
            dim_yellow: s("#B58C53"),
            dim_blue: s("#587CB3"),
            dim_magenta: s("#8D6EB3"),
            dim_cyan: s("#4C8C8C"),
            dim_white: s("#9A958A"),
        }
    } else {
        ColorPalette {
            foreground: s("#2B2A27"),
            background: s("#FBFAF6"),
            black: s("#2B2A27"),
            red: s("#C0453B"),
            green: s("#4E8C5A"),
            yellow: s("#B5852F"),
            blue: s("#3D6CB8"),
            magenta: s("#7E5DB0"),
            cyan: s("#3C8787"),
            white: s("#6B675E"),
            bright_black: s("#8A857B"),
            bright_red: s("#C0453B"),
            bright_green: s("#4E8C5A"),
            bright_yellow: s("#B5852F"),
            bright_blue: s("#3D6CB8"),
            bright_magenta: s("#7E5DB0"),
            bright_cyan: s("#3C8787"),
            bright_white: s("#2B2A27"),
            bright_foreground: Some(s("#2B2A27")),
            dim_foreground: s("#6B675E"),
            dim_black: s("#2B2A27"),
            dim_red: s("#A53C33"),
            dim_green: s("#42764C"),
            dim_yellow: s("#9A7128"),
            dim_blue: s("#345C9C"),
            dim_magenta: s("#6B4F96"),
            dim_cyan: s("#337373"),
            dim_white: s("#6B675E"),
        }
    }
}
