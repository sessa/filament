//! Settings builders for the embedded terminal (`filament-terminal`).
//!
//! The integrated terminal is our own [`filament_terminal`] widget, backed by
//! `alacritty_terminal` + `portable-pty` and rendered through iced's native
//! text/quad path. These helpers produce the backend settings for either a plain
//! shell or a Claude Code session, themed to match the app (vivid palette,
//! JetBrains Mono) and honoring the user's font size.

use std::collections::HashMap;
use std::path::PathBuf;

use filament_terminal::settings::{BackendSettings, FontSettings, Settings, ThemeSettings};
use filament_terminal::ColorPalette;
use iced::Font;

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
            env: child_env(),
            working_directory: cwd,
        },
    }
}

/// Extra environment for spawned terminals. The key one is an augmented `PATH`
/// (see [`augmented_path`]) so `claude` / `git` / `gh` resolve even when Filament
/// is launched from a GUI (Finder, the dock), where the inherited `PATH` is the
/// bare launchd default and excludes Homebrew / npm / Cargo bin dirs — the cause
/// of "Failed to spawn command 'claude': No such file or directory".
fn child_env() -> HashMap<String, String> {
    let mut env = HashMap::new();
    #[cfg(unix)]
    env.insert("PATH".to_string(), augmented_path());
    env
}

/// A `PATH` that unions, in priority order: the current process `PATH`, the login
/// shell's `PATH` (so nvm / asdf / Homebrew setups are honored), and a set of
/// well-known bin dirs GUI-launched apps usually miss. Computed once and cached.
#[cfg(unix)]
fn augmented_path() -> String {
    use std::sync::OnceLock;
    static CACHE: OnceLock<String> = OnceLock::new();
    CACHE.get_or_init(build_path).clone()
}

#[cfg(unix)]
fn build_path() -> String {
    let mut parts: Vec<String> = Vec::new();
    let add = |raw: &str, parts: &mut Vec<String>| {
        for seg in raw.split(':') {
            let seg = seg.trim();
            if !seg.is_empty() && !parts.iter().any(|p| p == seg) {
                parts.push(seg.to_string());
            }
        }
    };

    if let Ok(p) = std::env::var("PATH") {
        add(&p, &mut parts);
    }
    if let Some(p) = login_shell_path() {
        add(&p, &mut parts);
    }

    let mut well_known: Vec<String> = [
        "/opt/homebrew/bin",
        "/opt/homebrew/sbin",
        "/usr/local/bin",
        "/usr/local/sbin",
        "/usr/bin",
        "/bin",
        "/usr/sbin",
        "/sbin",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    if let Ok(home) = std::env::var("HOME") {
        for rel in [
            ".local/bin",
            ".npm-global/bin",
            ".cargo/bin",
            ".bun/bin",
            ".volta/bin",
            ".deno/bin",
            "go/bin",
        ] {
            well_known.push(format!("{home}/{rel}"));
        }
    }
    for dir in &well_known {
        add(dir, &mut parts);
    }

    parts.join(":")
}

/// Ask the user's login shell for its `PATH` (sourcing their profile/rc), so
/// tools installed via shell-managed version managers are visible. Best effort;
/// `printf %s` writes no trailing newline, so any rc chatter is on earlier lines
/// and we take only the last line.
#[cfg(unix)]
fn login_shell_path() -> Option<String> {
    let shell = std::env::var("SHELL").ok().filter(|s| !s.is_empty())?;
    let output = std::process::Command::new(shell)
        .args(["-lic", "printf %s \"$PATH\""])
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let path = stdout.rsplit('\n').next().unwrap_or("").trim().to_string();
    (!path.is_empty()).then_some(path)
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

/// The terminal palette: a warm off-white-on-near-black ground (to match the
/// app), but with **saturated, vivid** ANSI accent colors so program output —
/// `ls --color`, TUIs, the Claude UI — reads as colorful rather than washed out.
/// The earlier palette desaturated every hue and made everything look muted;
/// here the warm identity lives only in the foreground/background.
fn palette(dark: bool) -> ColorPalette {
    let s = |h: &str| h.to_string();
    if dark {
        ColorPalette {
            foreground: s("#ECEAE3"),
            background: s("#1B1A18"),
            black: s("#2B2824"),
            red: s("#F4564A"),
            green: s("#8FD46A"),
            yellow: s("#F2C14E"),
            blue: s("#6FA8FF"),
            magenta: s("#CB8CF0"),
            cyan: s("#4FD0D0"),
            white: s("#D8D4C8"),
            bright_black: s("#6E6A60"),
            bright_red: s("#FF7468"),
            bright_green: s("#B0E284"),
            bright_yellow: s("#FFD56B"),
            bright_blue: s("#8FBEFF"),
            bright_magenta: s("#DCA6F7"),
            bright_cyan: s("#74E6E6"),
            bright_white: s("#FBFAF6"),
            bright_foreground: Some(s("#FBFAF6")),
            dim_foreground: s("#9A958A"),
            dim_black: s("#1B1A18"),
            dim_red: s("#AB3C34"),
            dim_green: s("#64944A"),
            dim_yellow: s("#A98736"),
            dim_blue: s("#4D75B3"),
            dim_magenta: s("#8E62A8"),
            dim_cyan: s("#379090"),
            dim_white: s("#9A958A"),
        }
    } else {
        ColorPalette {
            foreground: s("#2B2A27"),
            background: s("#FBFAF6"),
            black: s("#2B2A27"),
            red: s("#D5392E"),
            green: s("#3E9B4F"),
            yellow: s("#C2860F"),
            blue: s("#1E6FE0"),
            magenta: s("#8B45C9"),
            cyan: s("#1C8C8C"),
            white: s("#6B675E"),
            bright_black: s("#8A857B"),
            bright_red: s("#E84C40"),
            bright_green: s("#49B25C"),
            bright_yellow: s("#D89A1C"),
            bright_blue: s("#2F82F2"),
            bright_magenta: s("#9C55DA"),
            bright_cyan: s("#23A0A0"),
            bright_white: s("#2B2A27"),
            bright_foreground: Some(s("#1C1B19")),
            dim_foreground: s("#6B675E"),
            dim_black: s("#2B2A27"),
            dim_red: s("#A82C23"),
            dim_green: s("#2F7A3D"),
            dim_yellow: s("#9A6A0C"),
            dim_blue: s("#1857B3"),
            dim_magenta: s("#6E37A0"),
            dim_cyan: s("#167070"),
            dim_white: s("#6B675E"),
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn augmented_path_includes_well_known_dirs_and_dedups() {
        let path = build_path();
        let segs: Vec<&str> = path.split(':').collect();
        // Well-known GUI-missing dirs are present.
        assert!(
            segs.contains(&"/usr/local/bin"),
            "missing /usr/local/bin: {path}"
        );
        assert!(segs.contains(&"/usr/bin"), "missing /usr/bin: {path}");
        // No duplicate segments.
        let mut sorted = segs.clone();
        sorted.sort_unstable();
        let before = sorted.len();
        sorted.dedup();
        assert_eq!(before, sorted.len(), "duplicate PATH segments: {path}");
        // The child env carries the augmented PATH.
        assert_eq!(child_env().get("PATH"), Some(&path));
    }
}
