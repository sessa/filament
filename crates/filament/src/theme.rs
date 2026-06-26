//! Color, surface, and shadow helpers for the "glass" look.
//!
//! Surfaces are intentionally translucent so that, on platforms where the window
//! is transparent + blurred (macOS vibrancy, KDE/Wayland blur), the desktop
//! shows through as a frosted backdrop. Elsewhere they read as tasteful
//! translucent panels over the app background.

use iced::{Color, Shadow, Theme, Vector};

use filament_core::{AgentColor, Scope};

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

pub fn danger() -> Color {
    Color::from_rgb8(0xE5, 0x48, 0x4D)
}

pub fn with_alpha(mut c: Color, a: f32) -> Color {
    c.a = a;
    c
}

fn is_dark(theme: &Theme) -> bool {
    theme.extended_palette().is_dark
}

/// Translucent app background (painted by the application style). Low alpha lets
/// OS blur show through.
pub fn app_background(theme: &Theme) -> Color {
    if is_dark(theme) {
        Color::from_rgba(0.055, 0.065, 0.095, 0.82)
    } else {
        Color::from_rgba(0.96, 0.97, 0.99, 0.86)
    }
}

/// Secondary/muted text color.
pub fn muted(theme: &Theme) -> Color {
    with_alpha(theme.palette().text, 0.55)
}

/// A faint translucent fill for cards.
pub fn surface(theme: &Theme) -> Color {
    if is_dark(theme) {
        Color::from_rgba(1.0, 1.0, 1.0, 0.05)
    } else {
        Color::from_rgba(0.0, 0.0, 0.0, 0.035)
    }
}

/// A slightly stronger fill for raised chrome (header, panels, inputs).
pub fn surface_strong(theme: &Theme) -> Color {
    if is_dark(theme) {
        Color::from_rgba(1.0, 1.0, 1.0, 0.08)
    } else {
        Color::from_rgba(0.0, 0.0, 0.0, 0.05)
    }
}

/// A hairline border color.
pub fn hairline(theme: &Theme) -> Color {
    with_alpha(theme.palette().text, 0.10)
}

/// Soft drop shadow for cards.
pub fn card_shadow() -> Shadow {
    Shadow {
        color: Color::from_rgba(0.0, 0.0, 0.0, 0.28),
        offset: Vector::new(0.0, 6.0),
        blur_radius: 22.0,
    }
}

/// Subtle shadow for small raised elements.
pub fn soft_shadow() -> Shadow {
    Shadow {
        color: Color::from_rgba(0.0, 0.0, 0.0, 0.18),
        offset: Vector::new(0.0, 2.0),
        blur_radius: 8.0,
    }
}
