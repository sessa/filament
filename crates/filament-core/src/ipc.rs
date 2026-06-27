//! The CLI ↔ app IPC protocol — crow's Unix-socket control surface, ported.
//!
//! A running Filament app listens on a Unix socket; the `filament <subcommand>`
//! CLI (and, via it, Claude Code's workspace skill) connects, sends one
//! newline-terminated JSON [`Request`], and reads one JSON [`Response`]. This is
//! how sessions, worktrees, and terminals can be driven programmatically — the
//! mechanism behind crow's `/crow-workspace`.
//!
//! [`dispatch`] performs all *store-backed* work (it loads/mutates/saves the
//! session store on disk) and returns both the [`Response`] to send back and an
//! optional [`Signal`] describing UI work the app must do (open/close/rename a
//! terminal, focus a session, type input, refresh). Keeping it here makes the
//! whole protocol unit-testable without a socket or a GUI.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::session::{Session, SessionLink, SessionState, SessionStore, TerminalRec};

/// A command sent by the CLI to the running app.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "cmd", rename_all = "kebab-case")]
pub enum Request {
    Ping,
    ListSessions,
    GetSession {
        id: String,
    },
    NewSession {
        title: String,
        #[serde(default)]
        base: Option<String>,
        #[serde(default)]
        issue: Option<String>,
        #[serde(default)]
        repo: Option<PathBuf>,
    },
    RenameSession {
        id: String,
        name: String,
    },
    SelectSession {
        id: String,
    },
    SetStatus {
        id: String,
        status: String,
    },
    DeleteSession {
        id: String,
        #[serde(default)]
        keep_worktree: bool,
    },
    SetTicket {
        id: String,
        #[serde(default)]
        url: Option<String>,
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        number: Option<u64>,
    },
    AddLink {
        id: String,
        label: String,
        url: String,
        #[serde(default)]
        kind: Option<String>,
    },
    ListLinks {
        id: String,
    },
    AddWorktree {
        id: String,
        repo: String,
        path: PathBuf,
        branch: String,
    },
    ListWorktrees {
        id: String,
    },
    NewTerminal {
        session: String,
        cwd: PathBuf,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        command: Option<String>,
    },
    ListTerminals {
        session: String,
    },
    CloseTerminal {
        session: String,
        terminal: String,
    },
    RenameTerminal {
        session: String,
        terminal: String,
        name: String,
    },
    Send {
        session: String,
        #[serde(default)]
        terminal: Option<String>,
        text: String,
    },
    HookEvent {
        session: String,
        event: String,
    },
}

/// The reply sent back to the CLI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "ok", rename_all = "kebab-case")]
#[allow(clippy::large_enum_variant)] // a one-shot CLI reply; not hot or stored
pub enum Response {
    Ok,
    Error { message: String },
    Sessions { sessions: Vec<Session> },
    Session { session: Session },
    Links { links: Vec<SessionLink> },
    Terminals { terminals: Vec<TerminalRec> },
    Created { id: String },
}

/// UI work the app must perform after a request (the store part is already done).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Signal {
    Refresh,
    Select {
        session: String,
    },
    OpenTerminal {
        session: String,
        terminal: TerminalRec,
    },
    CloseTerminal {
        session: String,
        terminal: String,
    },
    RenameTerminal {
        session: String,
        terminal: String,
        name: String,
    },
    Send {
        session: String,
        terminal: Option<String>,
        text: String,
    },
    Hook {
        session: String,
        event: String,
    },
}

/// The outcome of handling a [`Request`].
#[derive(Debug, Clone)]
pub struct Dispatch {
    pub response: Response,
    /// The store changed on disk — the app should reload it.
    pub changed: bool,
    /// UI side effect for the app to run.
    pub signal: Option<Signal>,
}

impl Dispatch {
    fn reply(response: Response) -> Dispatch {
        Dispatch {
            response,
            changed: false,
            signal: None,
        }
    }
    fn changed(response: Response) -> Dispatch {
        Dispatch {
            response,
            changed: true,
            signal: Some(Signal::Refresh),
        }
    }
    fn err(msg: impl Into<String>) -> Dispatch {
        Dispatch::reply(Response::Error {
            message: msg.into(),
        })
    }
}

static TERM_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Generate a process-unique terminal id.
fn new_terminal_id() -> String {
    let n = TERM_COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("term-{nanos:x}-{n}")
}

/// Default Unix-socket path for the app's IPC server.
pub fn default_socket_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("dev", "filament", "filament")
        .map(|d| d.data_local_dir().join("filament.sock"))
}

/// Handle one request against the store at `store_path`, returning the reply and
/// any UI signal. Pure with respect to everything except the on-disk store.
pub fn dispatch(req: Request, store_path: &std::path::Path) -> Dispatch {
    let mut store = SessionStore::load_at(store_path.to_path_buf());
    match req {
        Request::Ping => Dispatch::reply(Response::Ok),

        Request::ListSessions => Dispatch::reply(Response::Sessions {
            sessions: store.sessions.clone(),
        }),

        Request::GetSession { id } => match store.get(&id) {
            Some(s) => Dispatch::reply(Response::Session { session: s.clone() }),
            None => Dispatch::err(format!("no session: {id}")),
        },

        // Session creation that needs a worktree is delegated to the app (which
        // owns repo context); here we only report what's missing.
        Request::NewSession { .. } => Dispatch::err(
            "new-session over IPC requires the app; create from the UI or pass --repo via the app",
        ),

        Request::RenameSession { id, name } => match store.get_mut(&id) {
            Some(s) => {
                s.title = name;
                save_or_err(&store)
            }
            None => Dispatch::err(format!("no session: {id}")),
        },

        Request::SelectSession { id } => {
            if store.get(&id).is_some() {
                Dispatch {
                    response: Response::Ok,
                    changed: false,
                    signal: Some(Signal::Select { session: id }),
                }
            } else {
                Dispatch::err(format!("no session: {id}"))
            }
        }

        Request::SetStatus { id, status } => {
            let Some(state) = SessionState::parse(&status) else {
                return Dispatch::err(format!("unknown status: {status}"));
            };
            match store.get_mut(&id) {
                Some(s) => {
                    // Manual statuses pin; Working/Done follow derivation.
                    let manual = matches!(
                        state,
                        SessionState::Paused | SessionState::Archived | SessionState::Review
                    );
                    s.set_manual(manual.then_some(state));
                    if !manual {
                        s.state = state;
                    }
                    save_or_err(&store)
                }
                None => Dispatch::err(format!("no session: {id}")),
            }
        }

        Request::DeleteSession { id, keep_worktree } => {
            match crate::session::remove_session(&mut store, &id, !keep_worktree) {
                Ok(()) => save_or_err(&store),
                Err(e) => Dispatch::err(e.to_string()),
            }
        }

        Request::SetTicket {
            id,
            url,
            title,
            number,
        } => match store.get_mut(&id) {
            Some(s) => {
                let issue = s.issue.get_or_insert_with(Default::default);
                if let Some(u) = url {
                    issue.url = u;
                }
                if let Some(t) = title {
                    issue.title = t;
                }
                if let Some(n) = number {
                    issue.number = n;
                }
                if issue.state.is_empty() {
                    issue.state = "OPEN".into();
                }
                s.sync_state();
                save_or_err(&store)
            }
            None => Dispatch::err(format!("no session: {id}")),
        },

        Request::AddLink {
            id,
            label,
            url,
            kind,
        } => match store.get_mut(&id) {
            Some(s) => {
                s.links.push(SessionLink {
                    label,
                    url,
                    kind: kind.unwrap_or_else(|| "link".into()),
                });
                save_or_err(&store)
            }
            None => Dispatch::err(format!("no session: {id}")),
        },

        Request::ListLinks { id } => match store.get(&id) {
            Some(s) => Dispatch::reply(Response::Links {
                links: s.links.clone(),
            }),
            None => Dispatch::err(format!("no session: {id}")),
        },

        Request::AddWorktree {
            id,
            repo: _,
            path,
            branch,
        } => match store.get_mut(&id) {
            Some(s) => {
                s.worktree = path;
                s.branch = branch;
                save_or_err(&store)
            }
            None => Dispatch::err(format!("no session: {id}")),
        },

        Request::ListWorktrees { id } => match store.get(&id) {
            Some(s) => {
                // Represent the single tracked worktree as a one-element session list.
                Dispatch::reply(Response::Session { session: s.clone() })
            }
            None => Dispatch::err(format!("no session: {id}")),
        },

        Request::NewTerminal {
            session,
            cwd,
            name,
            command,
        } => match store.get_mut(&session) {
            Some(s) => {
                let rec = TerminalRec {
                    id: new_terminal_id(),
                    name: name.unwrap_or_else(|| "terminal".into()),
                    cwd,
                    kind: if command.is_some() {
                        "command"
                    } else {
                        "shell"
                    }
                    .into(),
                    command,
                };
                s.terminals.push(rec.clone());
                let _ = store.save();
                Dispatch {
                    response: Response::Created { id: rec.id.clone() },
                    changed: true,
                    signal: Some(Signal::OpenTerminal {
                        session,
                        terminal: rec,
                    }),
                }
            }
            None => Dispatch::err(format!("no session: {session}")),
        },

        Request::ListTerminals { session } => match store.get(&session) {
            Some(s) => Dispatch::reply(Response::Terminals {
                terminals: s.terminals.clone(),
            }),
            None => Dispatch::err(format!("no session: {session}")),
        },

        Request::CloseTerminal { session, terminal } => match store.get_mut(&session) {
            Some(s) => {
                s.terminals.retain(|t| t.id != terminal);
                let _ = store.save();
                Dispatch {
                    response: Response::Ok,
                    changed: true,
                    signal: Some(Signal::CloseTerminal { session, terminal }),
                }
            }
            None => Dispatch::err(format!("no session: {session}")),
        },

        Request::RenameTerminal {
            session,
            terminal,
            name,
        } => match store.get_mut(&session) {
            Some(s) => {
                if let Some(t) = s.terminals.iter_mut().find(|t| t.id == terminal) {
                    t.name = name.clone();
                }
                let _ = store.save();
                Dispatch {
                    response: Response::Ok,
                    changed: true,
                    signal: Some(Signal::RenameTerminal {
                        session,
                        terminal,
                        name,
                    }),
                }
            }
            None => Dispatch::err(format!("no session: {session}")),
        },

        Request::Send {
            session,
            terminal,
            text,
        } => {
            if store.get(&session).is_none() {
                return Dispatch::err(format!("no session: {session}"));
            }
            Dispatch {
                response: Response::Ok,
                changed: false,
                signal: Some(Signal::Send {
                    session,
                    terminal,
                    text,
                }),
            }
        }

        Request::HookEvent { session, event } => Dispatch {
            response: Response::Ok,
            changed: false,
            signal: Some(Signal::Hook { session, event }),
        },
    }
}

/// Persist `store`, turning an IO error into an error response.
fn save_or_err(store: &SessionStore) -> Dispatch {
    match store.save() {
        Ok(()) => Dispatch::changed(Response::Ok),
        Err(e) => Dispatch::err(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn seed(dir: &std::path::Path) -> PathBuf {
        let path = dir.join("sessions.json");
        let mut store = SessionStore::load_at(path.clone());
        store.sessions.push(Session {
            id: "s1".into(),
            title: "First".into(),
            branch: "feature".into(),
            base_branch: "main".into(),
            ..Session::default()
        });
        store.save().unwrap();
        path
    }

    #[test]
    fn request_roundtrips_through_json() {
        let req = Request::SetStatus {
            id: "s1".into(),
            status: "paused".into(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(req, back);
    }

    #[test]
    fn list_and_rename_and_status() {
        let dir = tempdir().unwrap();
        let path = seed(dir.path());

        let d = dispatch(Request::ListSessions, &path);
        assert!(matches!(d.response, Response::Sessions { sessions } if sessions.len() == 1));

        let d = dispatch(
            Request::RenameSession {
                id: "s1".into(),
                name: "Renamed".into(),
            },
            &path,
        );
        assert!(d.changed);
        let store = SessionStore::load_at(path.clone());
        assert_eq!(store.get("s1").unwrap().title, "Renamed");

        let d = dispatch(
            Request::SetStatus {
                id: "s1".into(),
                status: "paused".into(),
            },
            &path,
        );
        assert!(matches!(d.response, Response::Ok));
        let store = SessionStore::load_at(path.clone());
        assert_eq!(store.get("s1").unwrap().state, SessionState::Paused);
    }

    #[test]
    fn add_and_list_links() {
        let dir = tempdir().unwrap();
        let path = seed(dir.path());
        dispatch(
            Request::AddLink {
                id: "s1".into(),
                label: "Design".into(),
                url: "https://example.com".into(),
                kind: Some("design".into()),
            },
            &path,
        );
        let d = dispatch(Request::ListLinks { id: "s1".into() }, &path);
        match d.response {
            Response::Links { links } => {
                assert_eq!(links.len(), 1);
                assert_eq!(links[0].label, "Design");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn new_terminal_signals_open() {
        let dir = tempdir().unwrap();
        let path = seed(dir.path());
        let d = dispatch(
            Request::NewTerminal {
                session: "s1".into(),
                cwd: "/tmp".into(),
                name: Some("logs".into()),
                command: None,
            },
            &path,
        );
        assert!(matches!(d.response, Response::Created { .. }));
        assert!(matches!(d.signal, Some(Signal::OpenTerminal { .. })));
        let store = SessionStore::load_at(path);
        assert_eq!(store.get("s1").unwrap().terminals.len(), 1);
    }

    #[test]
    fn unknown_session_errors() {
        let dir = tempdir().unwrap();
        let path = seed(dir.path());
        let d = dispatch(Request::GetSession { id: "nope".into() }, &path);
        assert!(matches!(d.response, Response::Error { .. }));
    }
}
