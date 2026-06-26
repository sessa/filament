#![allow(dead_code)] // shared across test crates; not every test uses every helper

use std::path::{Path, PathBuf};

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

/// A throwaway directory tree for building bespoke discovery layouts
/// (managed/plugin/nested/local-settings) that the single static fixture can't
/// express. The temp dir is removed when the `Sandbox` is dropped, so keep it
/// bound for the lifetime of the test.
pub struct Sandbox {
    root: tempfile::TempDir,
}

impl Sandbox {
    pub fn new() -> Sandbox {
        Sandbox {
            root: tempfile::tempdir().expect("create temp dir"),
        }
    }

    pub fn path(&self) -> &Path {
        self.root.path()
    }

    /// Absolute path for a root-relative `rel`.
    pub fn join(&self, rel: &str) -> PathBuf {
        self.root.path().join(rel)
    }

    /// Write `contents` to the root-relative path `rel`, creating parent
    /// directories. Returns the absolute path written.
    pub fn write(&self, rel: &str, contents: &str) -> PathBuf {
        let path = self.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent dirs");
        }
        std::fs::write(&path, contents).expect("write file");
        path
    }

    /// Create an (empty) directory at the root-relative path `rel`.
    pub fn dir(&self, rel: &str) -> PathBuf {
        let path = self.join(rel);
        std::fs::create_dir_all(&path).expect("create dir");
        path
    }

    /// A minimal valid agent file (`name` + `description`).
    pub fn agent(&self, rel: &str, name: &str) -> PathBuf {
        self.write(
            rel,
            &format!("---\nname: {name}\ndescription: Agent {name}.\n---\nBody for {name}.\n"),
        )
    }

    /// A minimal valid `SKILL.md`.
    pub fn skill(&self, rel: &str, name: &str) -> PathBuf {
        self.write(
            rel,
            &format!("---\nname: {name}\ndescription: Skill {name}.\n---\n# {name}\n"),
        )
    }
}

impl Default for Sandbox {
    fn default() -> Self {
        Sandbox::new()
    }
}
