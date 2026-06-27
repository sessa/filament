//! Color, surface, and shadow helpers for Filament's "warm glass" look.
//!
//! The palette is tuned to feel at home next to Claude Code: warm, paper-and-ink
//! neutrals (not the usual cold blue-grays) with Claude's coral as the default
//! accent. Surfaces are intentionally translucent so that, on platforms where
//! the window is transparent + blurred (macOS vibrancy, KDE/Wayland blur), the
//! desktop shows through as a frosted backdrop. Elsewhere they read as tasteful
//! translucent panels over the warm app background.

use iced::{Color, Shadow, Theme, Vector};

use filament_core::{AgentColor, Scope};

use crate::prefs::{AccentChoice, ThemeMode};

// ---- type scale -------------------------------------------------------------
//
// A small, consistent set of font sizes used across the app. Kept deliberately
// tight — the desktop chrome reads better dense — with global zoom handled by
// the window scale factor (see `Density`).

pub const TEXT_TITLE: f32 = 15.0; // app + page titles
pub const TEXT_H1: f32 = 19.0; // inspector item name
pub const TEXT_H2: f32 = 16.0; // section / detail headings
pub const TEXT_BODY: f32 = 13.0; // descriptions, primary body
pub const TEXT_UI: f32 = 12.5; // buttons, inputs, rows
pub const TEXT_META: f32 = 11.5; // secondary metadata
pub const TEXT_LABEL: f32 = 10.5; // field labels, chips, group headers

// ---- corner radii -----------------------------------------------------------

pub const RADIUS_PANEL: f32 = 14.0;
pub const RADIUS_CARD: f32 = 12.0;
pub const RADIUS_CONTROL: f32 = 9.0;
pub const RADIUS_CHIP: f32 = 7.0;

// ---- theme construction -----------------------------------------------------

/// Resolve an accent to its sRGB color, nudged a touch deeper on light
/// backgrounds so it keeps enough contrast against the cream surfaces.
pub fn accent_color(accent: AccentChoice, dark: bool) -> Color {
    let (r, g, b) = accent.rgb();
    let c = Color::from_rgb8(r, g, b);
    if dark {
        c
    } else {
        darken(c, 0.12)
    }
}

/// Build the Iced [`Theme`] for the given mode + accent. The accent becomes the
/// palette's `primary`, so every widget that reads `palette().primary` picks it
/// up automatically.
pub fn build(mode: ThemeMode, accent: AccentChoice) -> Theme {
    let dark = mode.is_dark();
    let primary = accent_color(accent, dark);
    let (name, palette) = if dark {
        (
            "Filament Dark",
            iced::theme::Palette {
                background: Color::from_rgb8(0x21, 0x1F, 0x1D),
                text: Color::from_rgb8(0xED, 0xEA, 0xE3),
                primary,
                success: Color::from_rgb8(0x7F, 0xB0, 0x69),
                warning: Color::from_rgb8(0xE0, 0xAF, 0x68),
                danger: Color::from_rgb8(0xE5, 0x67, 0x5A),
            },
        )
    } else {
        (
            "Filament Light",
            iced::theme::Palette {
                background: Color::from_rgb8(0xFA, 0xF9, 0xF5),
                text: Color::from_rgb8(0x2B, 0x2A, 0x27),
                primary,
                success: Color::from_rgb8(0x4E, 0x8C, 0x5A),
                warning: Color::from_rgb8(0xB5, 0x85, 0x2F),
                danger: Color::from_rgb8(0xC0, 0x45, 0x3B),
            },
        )
    };
    Theme::custom(name.to_string(), palette)
}

// ---- semantic colors --------------------------------------------------------

/// Map an agent's declared color to an sRGB value.
pub fn agent_color(c: AgentColor) -> Color {
    let (r, g, b) = c.rgb();
    Color::from_rgb8(r, g, b)
}

/// A distinct accent per scope, used for the scope chips.
pub fn scope_accent(scope: Scope) -> Color {
    match scope {
        Scope::Managed => Color::from_rgb8(0xE8, 0x8B, 0x3C),
        Scope::Project => Color::from_rgb8(0x6F, 0x9B, 0xE0),
        Scope::User => Color::from_rgb8(0x7F, 0xB0, 0x69),
        Scope::Plugin => Color::from_rgb8(0xB1, 0x8A, 0xE0),
    }
}

pub fn danger() -> Color {
    Color::from_rgb8(0xE5, 0x67, 0x5A)
}

/// A warm amber, used for pending/in-review states.
pub fn amber() -> Color {
    Color::from_rgb8(0xE0, 0xAF, 0x68)
}

// ---- color utilities --------------------------------------------------------

pub fn with_alpha(mut c: Color, a: f32) -> Color {
    c.a = a;
    c
}

fn darken(c: Color, amount: f32) -> Color {
    let f = (1.0 - amount).clamp(0.0, 1.0);
    Color {
        r: c.r * f,
        g: c.g * f,
        b: c.b * f,
        a: c.a,
    }
}

fn is_dark(theme: &Theme) -> bool {
    theme.extended_palette().is_dark
}

// ---- surfaces ---------------------------------------------------------------

/// Translucent app background (painted by the application style). Low alpha lets
/// OS blur show through; the warm tint keeps it cohesive with Claude Code.
pub fn app_background(theme: &Theme) -> Color {
    if is_dark(theme) {
        Color::from_rgba(0.102, 0.098, 0.090, 0.86)
    } else {
        Color::from_rgba(0.969, 0.961, 0.945, 0.88)
    }
}

/// Secondary/muted text color.
pub fn muted(theme: &Theme) -> Color {
    with_alpha(theme.palette().text, 0.58)
}

/// Very faint text, for tertiary metadata.
pub fn faint(theme: &Theme) -> Color {
    with_alpha(theme.palette().text, 0.40)
}

/// A faint translucent fill for cards.
pub fn surface(theme: &Theme) -> Color {
    if is_dark(theme) {
        Color::from_rgba(1.0, 0.98, 0.95, 0.045)
    } else {
        Color::from_rgba(0.30, 0.26, 0.18, 0.035)
    }
}

/// A slightly stronger fill for raised chrome (header, panels, inputs).
pub fn surface_strong(theme: &Theme) -> Color {
    if is_dark(theme) {
        Color::from_rgba(1.0, 0.98, 0.95, 0.075)
    } else {
        Color::from_rgba(0.30, 0.26, 0.18, 0.055)
    }
}

/// A hairline border color.
pub fn hairline(theme: &Theme) -> Color {
    if is_dark(theme) {
        with_alpha(theme.palette().text, 0.12)
    } else {
        with_alpha(theme.palette().text, 0.10)
    }
}

// ---- shadows ----------------------------------------------------------------

/// Soft drop shadow for cards.
pub fn card_shadow() -> Shadow {
    Shadow {
        color: Color::from_rgba(0.0, 0.0, 0.0, 0.24),
        offset: Vector::new(0.0, 6.0),
        blur_radius: 22.0,
    }
}

/// Subtle shadow for small raised elements.
pub fn soft_shadow() -> Shadow {
    Shadow {
        color: Color::from_rgba(0.0, 0.0, 0.0, 0.16),
        offset: Vector::new(0.0, 2.0),
        blur_radius: 8.0,
    }
}
