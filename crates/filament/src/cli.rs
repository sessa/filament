//! Minimal, dependency-free argument parsing.
//!
//! ```text
//! filament [--workspace <dir>] [--home <dir>] [--no-user]
//! filament <dir>            # bare path is treated as the workspace
//! ```
//! `--home` overrides the user-config root so dev/tests never touch the real
//! `~/.claude`.

use std::collections::HashMap;
use std::path::PathBuf;

use filament_core::ipc;
use filament_core::{config::Config, DiscoveryOptions};

use crate::{ipc_server, scaffold};

pub struct Cli {
    pub workspace: Option<PathBuf>,
    pub home: Option<PathBuf>,
    pub include_user: bool,
    /// Preselect the first item whose name matches (handy for deep-linking and
    /// for headless screenshots).
    pub select: Option<String>,
    /// Prefill the search box (handy for deep-linking and headless screenshots).
    pub search: Option<String>,
    /// Start in the agent editor for the selected item (testing/screenshots).
    pub start_edit: bool,
    /// Start in the creation wizard (testing/screenshots).
    pub start_wizard: bool,
    /// Open the integrated terminal on launch (testing/screenshots).
    pub start_terminal: bool,
    /// Start in the Sessions section (testing/screenshots).
    pub start_sessions: bool,
    /// Start in the Settings section (testing/screenshots).
    pub start_settings: bool,
}

impl Cli {
    pub fn from_env() -> Cli {
        let mut workspace = None;
        let mut home = None;
        let mut include_user = true;
        let mut select = None;
        let mut search = None;
        let mut start_edit = false;
        let mut start_wizard = false;
        let mut start_terminal = false;
        let mut start_sessions = false;
        let mut start_settings = false;

        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--workspace" | "-w" => workspace = args.next().map(PathBuf::from),
                "--home" => home = args.next().map(PathBuf::from),
                "--select" | "-s" => select = args.next(),
                "--search" | "-q" => search = args.next(),
                "--edit" => start_edit = true,
                "--wizard" => start_wizard = true,
                "--terminal" => start_terminal = true,
                "--sessions" => start_sessions = true,
                "--settings" => start_settings = true,
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
            start_edit,
            start_wizard,
            start_terminal,
            start_sessions,
            start_settings,
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

/// Every crow-style control subcommand Filament understands.
const SUBCOMMANDS: &[&str] = &[
    "ping",
    "setup",
    "list-sessions",
    "get-session",
    "rename-session",
    "select-session",
    "set-status",
    "delete-session",
    "set-ticket",
    "add-link",
    "list-links",
    "add-worktree",
    "list-worktrees",
    "new-terminal",
    "list-terminals",
    "close-terminal",
    "rename-terminal",
    "send",
    "hook-event",
];

/// If the program was invoked as `filament <subcommand> …`, handle it (talking to
/// a running app over the IPC socket) and return the process exit code. Returns
/// `None` for the normal GUI launch.
pub fn run_subcommand() -> Option<i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let verb = args.first()?.clone();
    if !SUBCOMMANDS.contains(&verb.as_str()) {
        return None;
    }
    Some(dispatch(&verb, &args[1..]))
}

/// Parsed `--flag value` options plus trailing positional words.
struct Args {
    opts: HashMap<String, String>,
    flags: Vec<String>,
    positional: Vec<String>,
}

impl Args {
    fn parse(args: &[String]) -> Args {
        let bool_flags = ["--keep-worktree", "--json", "--primary", "--managed"];
        let mut opts = HashMap::new();
        let mut flags = Vec::new();
        let mut positional = Vec::new();
        let mut i = 0;
        while i < args.len() {
            let a = &args[i];
            if let Some(key) = a.strip_prefix("--") {
                if bool_flags.contains(&a.as_str()) {
                    flags.push(key.to_string());
                } else if let Some(v) = args.get(i + 1) {
                    opts.insert(key.to_string(), v.clone());
                    i += 1;
                } else {
                    flags.push(key.to_string());
                }
            } else {
                positional.push(a.clone());
            }
            i += 1;
        }
        Args {
            opts,
            flags,
            positional,
        }
    }

    fn get(&self, key: &str) -> Option<String> {
        self.opts.get(key).cloned()
    }
    fn has(&self, key: &str) -> bool {
        self.flags.iter().any(|f| f == key)
    }
    /// A session id from `--session` or `--id`.
    fn session(&self) -> Option<String> {
        self.get("session").or_else(|| self.get("id"))
    }
    /// A trailing positional value, or the named option, or the first positional.
    fn tail_or(&self, key: &str) -> Option<String> {
        self.get(key).or_else(|| self.positional.first().cloned())
    }
}

fn dispatch(verb: &str, rest: &[String]) -> i32 {
    let a = Args::parse(rest);

    if verb == "setup" {
        return run_setup(&a);
    }

    let req = match build_request(verb, &a) {
        Ok(r) => r,
        Err(msg) => {
            eprintln!("filament {verb}: {msg}");
            return 2;
        }
    };

    match ipc_server::send(&req) {
        Ok(resp) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&resp).unwrap_or_else(|_| "{}".into())
            );
            matches!(resp, ipc::Response::Error { .. }) as i32
        }
        Err(e) => {
            eprintln!("filament {verb}: {e}");
            1
        }
    }
}

fn require(value: Option<String>, what: &str) -> Result<String, String> {
    value.ok_or_else(|| format!("missing {what}"))
}

fn build_request(verb: &str, a: &Args) -> Result<ipc::Request, String> {
    Ok(match verb {
        "ping" => ipc::Request::Ping,
        "list-sessions" => ipc::Request::ListSessions,
        "get-session" => ipc::Request::GetSession {
            id: require(a.session(), "--session")?,
        },
        "rename-session" => ipc::Request::RenameSession {
            id: require(a.session(), "--session")?,
            name: require(a.tail_or("name"), "name")?,
        },
        "select-session" => ipc::Request::SelectSession {
            id: require(a.session(), "--session")?,
        },
        "set-status" => ipc::Request::SetStatus {
            id: require(a.session(), "--session")?,
            status: require(a.tail_or("status"), "status")?,
        },
        "delete-session" => ipc::Request::DeleteSession {
            id: require(a.session(), "--session")?,
            keep_worktree: a.has("keep-worktree"),
        },
        "set-ticket" => ipc::Request::SetTicket {
            id: require(a.session(), "--session")?,
            url: a.get("url"),
            title: a.get("title"),
            number: a.get("number").and_then(|n| n.parse().ok()),
        },
        "add-link" => ipc::Request::AddLink {
            id: require(a.session(), "--session")?,
            label: require(a.get("label"), "--label")?,
            url: require(a.get("url"), "--url")?,
            kind: a.get("type"),
        },
        "list-links" => ipc::Request::ListLinks {
            id: require(a.session(), "--session")?,
        },
        "add-worktree" => ipc::Request::AddWorktree {
            id: require(a.session(), "--session")?,
            repo: a.get("repo").unwrap_or_default(),
            path: PathBuf::from(require(a.get("path"), "--path")?),
            branch: require(a.get("branch"), "--branch")?,
        },
        "list-worktrees" => ipc::Request::ListWorktrees {
            id: require(a.session(), "--session")?,
        },
        "new-terminal" => ipc::Request::NewTerminal {
            session: require(a.session(), "--session")?,
            cwd: PathBuf::from(require(a.get("cwd"), "--cwd")?),
            name: a.get("name"),
            command: a.get("command"),
        },
        "list-terminals" => ipc::Request::ListTerminals {
            session: require(a.session(), "--session")?,
        },
        "close-terminal" => ipc::Request::CloseTerminal {
            session: require(a.session(), "--session")?,
            terminal: require(a.get("terminal"), "--terminal")?,
        },
        "rename-terminal" => ipc::Request::RenameTerminal {
            session: require(a.session(), "--session")?,
            terminal: require(a.get("terminal"), "--terminal")?,
            name: require(a.tail_or("name"), "name")?,
        },
        "send" => ipc::Request::Send {
            session: require(a.session(), "--session")?,
            terminal: a.get("terminal"),
            text: if a.positional.is_empty() {
                require(a.get("text"), "text")?
            } else {
                a.positional.join(" ")
            },
        },
        "hook-event" => ipc::Request::HookEvent {
            session: require(a.session(), "--session")?,
            event: require(a.tail_or("event"), "--event")?,
        },
        other => return Err(format!("unknown subcommand: {other}")),
    })
}

/// `filament setup [--dev-root PATH]` — initialize config and scaffold the
/// workspace skill (works without a running app, like `crow setup`).
fn run_setup(a: &Args) -> i32 {
    let mut cfg = Config::load();
    let dev_root = a
        .get("dev-root")
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok());
    cfg.dev_root = dev_root.clone();
    cfg.initialized = true;
    if let Err(e) = cfg.save() {
        eprintln!("filament setup: could not save config: {e}");
        return 1;
    }
    if let Some(root) = &dev_root {
        match scaffold::write_workspace_skill(root) {
            Ok(path) => println!("Wrote workspace skill: {}", path.display()),
            Err(e) => eprintln!("filament setup: could not scaffold skill: {e}"),
        }
    }
    if let Some(path) = &cfg.path {
        println!("Configuration written to {}", path.display());
    }
    println!("Filament is set up. Launch the app and open the Sessions section.");
    0
}
