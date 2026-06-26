//! Filament — a desktop dashboard for Claude Code configuration.
//!
//! The binary is a thin Iced shell over `filament-core`: it loads a
//! [`filament_core::Workspace`] and renders agents, skills, commands, MCP
//! servers, and settings in a polished three-pane layout.

// On Windows, don't pop a console window for the GUI app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod cli;
mod inspector;
mod search;
mod sidebar;
mod theme;
mod widgets;

use app::App;

fn main() -> iced::Result {
    iced::application(App::new, App::update, App::view)
        .title(App::title)
        .theme(App::theme)
        .window_size((1200.0, 780.0))
        .antialiasing(true)
        .run()
}
