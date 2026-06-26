//! Root finding and recursive scanning.
//!
//! Builds the ordered list of configuration roots (project `.claude/` dirs
//! nearest-first, then the user `~/.claude/`, then plugins), assigns each a
//! numeric precedence, scans every root for the file kinds it can contain, and
//! returns a resolved [`Catalog`]. A numeric precedence (rather than scope
//! alone) lets nearest-project definitions win over farther ones within the same
//! [`Scope`].

use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::model::{Catalog, Entry};
use crate::parse;
use crate::scope::Scope;
use crate::workspace::DiscoveryOptions;

struct Root {
    scope: Scope,
    precedence: u32,
    /// The `.claude` directory (or a plugin directory) holding agents/skills/etc.
    claude_dir: PathBuf,
    /// The directory where `.mcp.json` lives (parent of `.claude`, or the plugin dir).
    base_dir: PathBuf,
}

/// Discover and resolve all configuration reachable from `opts`.
pub fn discover(opts: &DiscoveryOptions) -> Catalog {
    let mut roots: Vec<Root> = Vec::new();
    let mut precedence: u32 = 0;
    let next = |p: &mut u32| {
        let v = *p;
        *p += 1;
        v
    };

    // Managed scope, if an explicit directory was provided (highest precedence).
    if let Some(managed) = &opts.managed {
        if managed.is_dir() {
            roots.push(Root {
                scope: Scope::Managed,
                precedence: next(&mut precedence),
                claude_dir: managed.clone(),
                base_dir: managed.parent().unwrap_or(managed).to_path_buf(),
            });
        }
    }

    // Project roots — nearest `.claude` to the workspace wins.
    if let Some(ws) = &opts.workspace {
        for base in ancestors_with_claude(ws) {
            roots.push(Root {
                scope: Scope::Project,
                precedence: next(&mut precedence),
                claude_dir: base.join(".claude"),
                base_dir: base,
            });
        }
    }

    // User root + plugins.
    if opts.include_user {
        if let Some(home) = opts.home_dir() {
            let claude = home.join(".claude");
            if claude.is_dir() {
                roots.push(Root {
                    scope: Scope::User,
                    precedence: next(&mut precedence),
                    claude_dir: claude.clone(),
                    base_dir: home.clone(),
                });
            }
            let plugins = claude.join("plugins");
            if plugins.is_dir() {
                for plugin_dir in subdirs(&plugins) {
                    roots.push(Root {
                        scope: Scope::Plugin,
                        precedence: next(&mut precedence),
                        claude_dir: plugin_dir.clone(),
                        base_dir: plugin_dir,
                    });
                }
            }
        }
    }

    let mut entries: Vec<Entry> = Vec::new();
    for root in &roots {
        scan_root(root, &mut entries);
    }
    Catalog::new(entries)
}

fn scan_root(root: &Root, out: &mut Vec<Entry>) {
    // Agents: <root>/agents/**/*.md
    for path in md_files(&root.claude_dir.join("agents")) {
        out.push(parse::parse_agent(&path, root.scope, root.precedence));
    }
    // Skills: <root>/skills/**/SKILL.md
    for path in named_files(&root.claude_dir.join("skills"), "SKILL.md") {
        out.push(parse::parse_skill(&path, root.scope, root.precedence));
    }
    // Commands: <root>/commands/**/*.md
    for path in md_files(&root.claude_dir.join("commands")) {
        out.push(parse::parse_command(&path, root.scope, root.precedence));
    }
    // Settings: <root>/settings.json and settings.local.json
    for name in ["settings.json", "settings.local.json"] {
        let path = root.claude_dir.join(name);
        if path.is_file() {
            out.push(parse::parse_settings(&path, root.scope, root.precedence));
        }
    }
    // MCP servers: <base>/.mcp.json
    let mcp = root.base_dir.join(".mcp.json");
    if mcp.is_file() {
        out.extend(parse::parse_mcp_file(&mcp, root.scope, root.precedence));
    }
}

/// All ancestor directories of `start` (inclusive) that contain a `.claude`
/// directory, nearest first. The walk stops at the enclosing git repository root
/// (the first ancestor containing `.git`), so project discovery never escapes
/// the repo into unrelated configs higher up the tree.
fn ancestors_with_claude(start: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for anc in start.ancestors() {
        if anc.join(".claude").is_dir() {
            out.push(anc.to_path_buf());
        }
        if anc.join(".git").exists() {
            break;
        }
    }
    out
}

/// Recursively collect `*.md` files under `dir`, sorted for determinism.
fn md_files(dir: &Path) -> Vec<PathBuf> {
    collect(dir, |p| {
        p.extension()
            .map(|e| e.eq_ignore_ascii_case("md"))
            .unwrap_or(false)
    })
}

/// Recursively collect files whose file name equals `name` (case-insensitive).
fn named_files(dir: &Path, name: &str) -> Vec<PathBuf> {
    collect(dir, |p| {
        p.file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.eq_ignore_ascii_case(name))
            .unwrap_or(false)
    })
}

fn collect(dir: &Path, keep: impl Fn(&Path) -> bool) -> Vec<PathBuf> {
    if !dir.is_dir() {
        return Vec::new();
    }
    let mut out: Vec<PathBuf> = WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .map(|e| e.path().to_path_buf())
        .filter(|p| keep(p))
        .collect();
    out.sort();
    out
}

fn subdirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = rd
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    out.sort();
    out
}
