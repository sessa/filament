#![allow(dead_code)] // shared across test crates; not every test uses every helper

use std::path::PathBuf;

use filament_core::{DiscoveryOptions, Workspace};

pub fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Load the `workspace_a` fixture with the `home` fixture as the user scope.
pub fn load_workspace_a() -> Workspace {
    let f = fixtures();
    let opts = DiscoveryOptions {
        workspace: Some(f.join("workspace_a")),
        home: Some(f.join("home")),
        managed: None,
        include_user: true,
    };
    Workspace::load(opts)
}
