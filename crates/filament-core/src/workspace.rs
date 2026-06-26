//! High-level entry point the UI drives: configure roots, load, and rescan.

use std::path::PathBuf;

use crate::discovery::discover;
use crate::model::Catalog;

/// Where to look for configuration.
#[derive(Debug, Clone)]
pub struct DiscoveryOptions {
    /// A project directory to scan upward from for `.claude/` dirs.
    pub workspace: Option<PathBuf>,
    /// Override for the home directory (so tests/dev never touch the real
    /// `~/.claude`). When `None`, the OS home directory is used.
    pub home: Option<PathBuf>,
    /// An explicit managed-settings `.claude` directory, if any.
    pub managed: Option<PathBuf>,
    /// Whether to include the user scope (`~/.claude`) and plugins.
    pub include_user: bool,
}

impl Default for DiscoveryOptions {
    fn default() -> Self {
        DiscoveryOptions {
            workspace: None,
            home: None,
            managed: None,
            include_user: true,
        }
    }
}

impl DiscoveryOptions {
    /// The effective home directory: the override, else the OS home.
    pub fn home_dir(&self) -> Option<PathBuf> {
        self.home
            .clone()
            .or_else(|| directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf()))
    }

    pub fn with_workspace(mut self, path: impl Into<PathBuf>) -> Self {
        self.workspace = Some(path.into());
        self
    }

    pub fn with_home(mut self, path: impl Into<PathBuf>) -> Self {
        self.home = Some(path.into());
        self
    }
}

/// A loaded configuration set plus the options used to produce it.
#[derive(Debug, Clone)]
pub struct Workspace {
    pub options: DiscoveryOptions,
    pub catalog: Catalog,
}

impl Workspace {
    /// Discover and resolve everything reachable from `options`.
    pub fn load(options: DiscoveryOptions) -> Workspace {
        let catalog = discover(&options);
        Workspace { options, catalog }
    }

    /// Re-run discovery with the same options (after a file change).
    pub fn rescan(&mut self) {
        self.catalog = discover(&self.options);
    }
}
