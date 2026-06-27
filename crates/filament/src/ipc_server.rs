//! The in-app IPC server (Unix socket) and a tiny blocking client.
//!
//! When the GUI is running it listens on a Unix socket so the `filament`
//! subcommands — and, through them, Claude Code's workspace skill — can drive
//! sessions, worktrees, and terminals (crow's socket control surface). Each
//! connection carries one newline-terminated JSON [`ipc::Request`]; the server
//! handles the store-backed part via [`ipc::dispatch`], writes back the
//! [`ipc::Response`], and forwards any UI [`ipc::Signal`] into the Iced runtime
//! as a [`Message::Ipc`]. The socket is Unix-only; on other platforms the
//! subscription is inert (the rest of the app is unaffected).

use std::path::PathBuf;

use iced::Subscription;

use filament_core::ipc;

use crate::app::Message;

/// A subscription that serves IPC requests for the store at `store_path`,
/// emitting a [`Message::Ipc`] for each request's UI signal.
pub fn subscription(store_path: PathBuf) -> Subscription<Message> {
    #[cfg(unix)]
    {
        let Some(sock) = ipc::default_socket_path() else {
            return Subscription::none();
        };
        Subscription::run_with((sock, store_path), build)
    }
    #[cfg(not(unix))]
    {
        let _ = store_path;
        Subscription::none()
    }
}

#[cfg(unix)]
#[allow(clippy::ptr_arg)]
fn build(key: &(PathBuf, PathBuf)) -> impl iced::futures::Stream<Item = Message> {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixListener;

    let (sock, store_path) = key.clone();
    iced::stream::channel(
        64,
        move |sender: iced::futures::channel::mpsc::Sender<Message>| async move {
            std::thread::spawn(move || {
                if let Some(parent) = sock.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                // A stale socket file from a previous run blocks binding; clear it.
                let _ = std::fs::remove_file(&sock);
                let Ok(listener) = UnixListener::bind(&sock) else {
                    return;
                };
                let mut tx = sender;
                for stream in listener.incoming() {
                    let Ok(mut stream) = stream else { continue };
                    let mut reader = BufReader::new(match stream.try_clone() {
                        Ok(s) => s,
                        Err(_) => continue,
                    });
                    let mut line = String::new();
                    if reader.read_line(&mut line).is_err() || line.trim().is_empty() {
                        continue;
                    }
                    let dispatch = match serde_json::from_str::<ipc::Request>(line.trim()) {
                        Ok(req) => ipc::dispatch(req, &store_path),
                        Err(e) => ipc::Dispatch {
                            response: ipc::Response::Error {
                                message: format!("bad request: {e}"),
                            },
                            changed: false,
                            signal: None,
                        },
                    };
                    if let Ok(mut json) = serde_json::to_string(&dispatch.response) {
                        json.push('\n');
                        let _ = stream.write_all(json.as_bytes());
                        let _ = stream.flush();
                    }
                    if dispatch.changed {
                        let _ = tx.try_send(Message::Ipc(ipc::Signal::Refresh));
                    }
                    if let Some(signal) = dispatch.signal {
                        if signal != ipc::Signal::Refresh {
                            let _ = tx.try_send(Message::Ipc(signal));
                        }
                    }
                }
            });
            std::future::pending::<()>().await;
        },
    )
}

/// Send one request to a running app over the Unix socket and read its response.
#[cfg(unix)]
pub fn send(req: &ipc::Request) -> Result<ipc::Response, String> {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;

    let sock = ipc::default_socket_path().ok_or("no socket path")?;
    let mut stream = UnixStream::connect(&sock)
        .map_err(|e| format!("could not connect to a running Filament ({e}). Is the app open?"))?;
    let mut json = serde_json::to_string(req).map_err(|e| e.to_string())?;
    json.push('\n');
    stream
        .write_all(json.as_bytes())
        .map_err(|e| e.to_string())?;
    stream.flush().map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).map_err(|e| e.to_string())?;
    serde_json::from_str(line.trim()).map_err(|e| e.to_string())
}

#[cfg(not(unix))]
pub fn send(_req: &ipc::Request) -> Result<ipc::Response, String> {
    Err("the Filament CLI requires a Unix socket (not available on this platform)".into())
}
