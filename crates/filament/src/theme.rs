//! Color/icon helpers shared by the views. Kept small in M2; palettes and the
//! icon font arrive in M4.

use iced::{Color, Theme};

use filament_core::{AgentColor, ItemKind, Scope};

/// Map an agent's declared color to an sRGB value.
pub fn agent_color(c: AgentColor) -> Color {
    let (r, g, b) = c.rgb();
    Color::from_rgb8(r, g, b)
}

/// A distinct accent per scope, used for the scope chips.
pub fn scope_accent(scope: Scope) -> Color {
    match scope {
        Scope::Managed => Color::from_rgb8(0xE8, 0x8B, 0x3C),
        Scope::Project => Color::from_rgb8(0x4C, 0x8B, 0xF5),
        Scope::User => Color::from_rgb8(0x3F, 0xB9, 0x50),
        Scope::Plugin => Color::from_rgb8(0x9B, 0x59, 0xD6),
    }
}

/// A placeholder glyph per item kind (replaced by an icon font in M4).
pub fn kind_glyph(kind: ItemKind) -> &'static str {
    match kind {
        ItemKind::Agent => "◆",
        ItemKind::Skill => "✦",
        ItemKind::Command => "⌘",
        ItemKind::McpServer => "⇄",
        ItemKind::Settings => "⚙",
    }
}

/// A red used for error badges.
pub fn danger() -> Color {
    Color::from_rgb8(0xE5, 0x48, 0x4D)
}

pub fn with_alpha(mut c: Color, a: f32) -> Color {
    c.a = a;
    c
}

/// Secondary/muted text color for the current theme.
pub fn muted(theme: &Theme) -> Color {
    with_alpha(theme.palette().text, 0.55)
}

/// A subtle raised-surface color for cards.
pub fn surface(theme: &Theme) -> Color {
    theme.extended_palette().background.weak.color
}

/// A hairline border color.
pub fn hairline(theme: &Theme) -> Color {
    with_alpha(theme.palette().text, 0.10)
}
