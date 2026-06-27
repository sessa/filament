//! Filament — a desktop dashboard for Claude Code configuration.
//!
//! The binary is a thin Iced shell over `filament-core`: it loads a
//! [`filament_core::Workspace`] and renders agents, skills, commands, MCP
//! servers, and settings in a polished three-pane layout.

// On Windows, don't pop a console window for the GUI app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod cli;
mod editor;
mod icon;
mod inspector;
mod ipc_server;
mod prefs;
mod scaffold;
mod search;
mod sessions;
mod settingsview;
mod sidebar;
mod terminal;
mod theme;
mod watcher;
mod widgets;
mod wizard;

use app::App;

fn main() -> iced::Result {
    // `filament <subcommand> …` drives a running app over the IPC socket (crow's
    // CLI surface) and exits without launching a second GUI.
    if let Some(code) = cli::run_subcommand() {
        std::process::exit(code);
    }

    iced::application(App::new, App::update, App::view)
        .title(App::title)
        .theme(App::theme)
        .style(App::app_style)
        .scale_factor(App::scale_factor)
        .subscription(App::subscription)
        .font(include_bytes!("../assets/fonts/Inter.ttf").as_slice())
        .font(include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf").as_slice())
        .font(include_bytes!("../assets/icons/phosphor.ttf").as_slice())
        .default_font(iced::Font::with_name("Inter"))
        .window(iced::window::Settings {
            size: iced::Size::new(1240.0, 820.0),
            min_size: Some(iced::Size::new(880.0, 580.0)),
            transparent: true,
            blur: true,
            ..Default::default()
        })
        .antialiasing(true)
        .run()
}
