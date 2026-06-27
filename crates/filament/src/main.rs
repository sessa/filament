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
mod logging;
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

    // Logging to a file (in the OS data dir) + stderr, so GUI launches leave a
    // trace. `RUST_LOG=debug` raises verbosity for chasing render/wgpu issues.
    let log_path = logging::init();
    log::info!(
        "Filament v{} starting{}",
        env!("CARGO_PKG_VERSION"),
        log_path
            .as_ref()
            .map(|p| format!(" — logging to {}", p.display()))
            .unwrap_or_default()
    );

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
        .window(window_settings())
        .antialiasing(true)
        .run()
}

/// Window chrome for the glass UI.
///
/// The app paints its own rounded, translucent frame in `app::App::view`, so on
/// macOS we dissolve the native title bar into it: the titlebar is made
/// transparent, its text hidden, and the content drawn full-height behind it,
/// leaving only the traffic-light buttons floating over the glass. Other
/// platforms keep their native decorations.
fn window_settings() -> iced::window::Settings {
    let settings = iced::window::Settings {
        size: iced::Size::new(1240.0, 820.0),
        min_size: Some(iced::Size::new(880.0, 580.0)),
        transparent: true,
        blur: true,
        ..Default::default()
    };

    #[cfg(target_os = "macos")]
    let settings = iced::window::Settings {
        platform_specific: iced::window::settings::PlatformSpecific {
            title_hidden: true,
            titlebar_transparent: true,
            fullsize_content_view: true,
        },
        ..settings
    };

    settings
}
