//! Minimal, dependency-free argument parsing.
//!
//! ```text
//! filament [--workspace <dir>] [--home <dir>] [--no-user]
//! filament <dir>            # bare path is treated as the workspace
//! ```
//! `--home` overrides the user-config root so dev/tests never touch the real
//! `~/.claude`.

use std::path::PathBuf;

use filament_core::DiscoveryOptions;

pub struct Cli {
    pub workspace: Option<PathBuf>,
    pub home: Option<PathBuf>,
    pub include_user: bool,
    /// Preselect the first item whose name matches (handy for deep-linking and
    /// for headless screenshots).
    pub select: Option<String>,
    /// Prefill the search box (handy for deep-linking and headless screenshots).
    pub search: Option<String>,
}

impl Cli {
    pub fn from_env() -> Cli {
        let mut workspace = None;
        let mut home = None;
        let mut include_user = true;
        let mut select = None;
        let mut search = None;

        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--workspace" | "-w" => workspace = args.next().map(PathBuf::from),
                "--home" => home = args.next().map(PathBuf::from),
                "--select" | "-s" => select = args.next(),
                "--search" | "-q" => search = args.next(),
                "--no-user" => include_user = false,
                other if !other.starts_with('-') && workspace.is_none() => {
                    workspace = Some(PathBuf::from(other));
                }
                _ => {}
            }
        }

        if workspace.is_none() {
            workspace = std::env::current_dir().ok();
        }

        Cli {
            workspace,
            home,
            include_user,
            select,
            search,
        }
    }

    pub fn options(&self) -> DiscoveryOptions {
        DiscoveryOptions {
            workspace: self.workspace.clone(),
            home: self.home.clone(),
            managed: None,
            include_user: self.include_user,
        }
    }
}
