//! Persisted application preferences.
//!
//! These are *app-level* settings (appearance, density, terminal, session
//! defaults) — distinct from the user's Claude Code `settings.json` that the
//! Config section reads/edits. They're stored as a small JSON file in the OS
//! data directory so they survive restarts, and loaded once at startup.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Which color scheme to render.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    #[default]
    Dark,
    Light,
    /// The cool, navy "Ayu" palette (Mirage variant).
    Ayu,
}

impl ThemeMode {
    pub const ALL: [ThemeMode; 3] = [ThemeMode::Dark, ThemeMode::Light, ThemeMode::Ayu];

    /// Whether this theme renders on a dark background (used for terminal tint
    /// and accent contrast).
    pub fn is_dark(self) -> bool {
        matches!(self, ThemeMode::Dark | ThemeMode::Ayu)
    }

    /// The next theme in the header's quick-cycle order.
    pub fn next(self) -> ThemeMode {
        match self {
            ThemeMode::Dark => ThemeMode::Light,
            ThemeMode::Light => ThemeMode::Ayu,
            ThemeMode::Ayu => ThemeMode::Dark,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ThemeMode::Dark => "Dark",
            ThemeMode::Light => "Light",
            ThemeMode::Ayu => "Ayu",
        }
    }
}

impl std::fmt::Display for ThemeMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// The accent / primary color. Defaults to Claude's coral.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AccentChoice {
    #[default]
    Coral,
    Blue,
    Green,
    Purple,
    Amber,
    Rose,
}

impl AccentChoice {
    pub const ALL: [AccentChoice; 6] = [
        AccentChoice::Coral,
        AccentChoice::Blue,
        AccentChoice::Green,
        AccentChoice::Purple,
        AccentChoice::Amber,
        AccentChoice::Rose,
    ];

    /// The base sRGB for this accent (tuned for dark surfaces).
    pub fn rgb(self) -> (u8, u8, u8) {
        match self {
            AccentChoice::Coral => (0xD9, 0x77, 0x57),
            AccentChoice::Blue => (0x5B, 0x8D, 0xEF),
            AccentChoice::Green => (0x5F, 0xB3, 0x7A),
            AccentChoice::Purple => (0x9B, 0x7B, 0xD4),
            AccentChoice::Amber => (0xE0, 0xAF, 0x68),
            AccentChoice::Rose => (0xE0, 0x6B, 0x80),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            AccentChoice::Coral => "Coral",
            AccentChoice::Blue => "Blue",
            AccentChoice::Green => "Green",
            AccentChoice::Purple => "Purple",
            AccentChoice::Amber => "Amber",
            AccentChoice::Rose => "Rose",
        }
    }
}

impl std::fmt::Display for AccentChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// Overall UI density, applied as a global window scale factor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Density {
    Compact,
    #[default]
    Cozy,
    Comfortable,
    Spacious,
}

impl Density {
    pub const ALL: [Density; 4] = [
        Density::Compact,
        Density::Cozy,
        Density::Comfortable,
        Density::Spacious,
    ];

    /// The window scale factor for this density. Values below 1.0 make the whole
    /// UI tighter (addressing "everything seems a bit big").
    pub fn scale(self) -> f32 {
        match self {
            Density::Compact => 0.85,
            Density::Cozy => 0.92,
            Density::Comfortable => 1.0,
            Density::Spacious => 1.1,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Density::Compact => "Compact",
            Density::Cozy => "Cozy",
            Density::Comfortable => "Comfortable",
            Density::Spacious => "Spacious",
        }
    }
}

impl std::fmt::Display for Density {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// Smallest / largest terminal font size we let the user dial to.
pub const TERM_FONT_MIN: f32 = 9.0;
pub const TERM_FONT_MAX: f32 = 22.0;

/// All persisted preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub theme: ThemeMode,
    pub accent: AccentChoice,
    pub density: Density,
    /// Embedded terminal font size, in points.
    pub terminal_font_size: f32,
    /// Override for the shell launched in the terminal (empty = `$SHELL`).
    pub shell: String,
    /// Show sessions from every repository on the board, not just the active one.
    pub show_all_sessions: bool,
    /// The repository to attach new sessions to (when set and valid).
    pub default_repo: Option<PathBuf>,

    /// Where this was loaded from / saves to. Not serialized.
    #[serde(skip)]
    pub path: Option<PathBuf>,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            theme: ThemeMode::default(),
            accent: AccentChoice::default(),
            density: Density::default(),
            terminal_font_size: 13.0,
            shell: String::new(),
            show_all_sessions: true,
            default_repo: None,
            path: None,
        }
    }
}

impl Prefs {
    /// The default preferences file location in the OS data directory.
    pub fn default_path() -> Option<PathBuf> {
        directories::ProjectDirs::from("dev", "filament", "filament")
            .map(|d| d.data_local_dir().join("prefs.json"))
    }

    /// Load preferences, falling back to defaults when absent or unreadable.
    pub fn load() -> Prefs {
        let path = Self::default_path();
        let mut prefs = path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str::<Prefs>(&s).ok())
            .unwrap_or_default();
        prefs.path = path;
        prefs.terminal_font_size = prefs.terminal_font_size.clamp(TERM_FONT_MIN, TERM_FONT_MAX);
        prefs
    }

    /// Persist to disk (best effort; errors are ignored so the UI never blocks).
    pub fn save(&self) {
        let Some(path) = &self.path else { return };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(data) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, data);
        }
    }

    pub fn bump_terminal_font(&mut self, delta: f32) {
        self.terminal_font_size =
            (self.terminal_font_size + delta).clamp(TERM_FONT_MIN, TERM_FONT_MAX);
    }
}

/// Messages that mutate preferences from the Settings UI.
#[derive(Debug, Clone)]
pub enum PrefMsg {
    SetTheme(ThemeMode),
    SetAccent(AccentChoice),
    SetDensity(Density),
    TermFontDelta(f32),
    ShellChanged(String),
    ToggleShowAll(bool),
}
