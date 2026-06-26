//! Phosphor icon font glyphs.
//!
//! The `Phosphor.ttf` icon font is bundled and loaded in `main`. Each constant
//! is the Private-Use-Area codepoint for one icon (extracted from Phosphor's
//! stylesheet). `icon(cp)` builds a `Text` widget set in the icon font.

use iced::widget::{text, Text};
use iced::Font;

use filament_core::{ItemKind, Scope};

pub const FONT: Font = Font::with_name("Phosphor");

// Item kinds
pub const AGENT: char = '\u{e762}'; // robot
pub const SKILL: char = '\u{e6a2}'; // sparkle
pub const COMMAND: char = '\u{eae8}'; // terminal-window
pub const MCP: char = '\u{eb5a}'; // plugs-connected
pub const SETTINGS: char = '\u{e270}'; // gear

// Actions / chrome
pub const SEARCH: char = '\u{e30c}'; // magnifying-glass
pub const RUN: char = '\u{e3d0}'; // play
pub const EDIT: char = '\u{e3b4}'; // pencil-simple
pub const SOURCE: char = '\u{e1bc}'; // code
pub const NEW: char = '\u{e3d4}'; // plus
pub const TERMINAL: char = '\u{eae8}'; // terminal-window
pub const REFRESH: char = '\u{e094}'; // arrows-clockwise
pub const SUN: char = '\u{e472}';
pub const MOON: char = '\u{e330}';
pub const CLOSE: char = '\u{e4f6}'; // x
pub const SAVE: char = '\u{e710}'; // files

// Status / scope
pub const WARNING: char = '\u{e4e0}'; // warning
pub const PROJECT: char = '\u{e24a}'; // folder
pub const USER: char = '\u{e2c2}'; // house
pub const MANAGED: char = '\u{e40c}'; // shield-check
pub const PLUGIN: char = '\u{e596}'; // puzzle-piece

/// A `Text` widget showing `cp` in the icon font.
pub fn icon<'a>(cp: char) -> Text<'a> {
    text(cp.to_string()).font(FONT)
}

pub fn kind(kind: ItemKind) -> char {
    match kind {
        ItemKind::Agent => AGENT,
        ItemKind::Skill => SKILL,
        ItemKind::Command => COMMAND,
        ItemKind::McpServer => MCP,
        ItemKind::Settings => SETTINGS,
    }
}

pub fn scope(scope: Scope) -> char {
    match scope {
        Scope::Managed => MANAGED,
        Scope::Project => PROJECT,
        Scope::User => USER,
        Scope::Plugin => PLUGIN,
    }
}
