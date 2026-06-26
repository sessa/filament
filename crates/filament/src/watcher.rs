//! Filesystem watching bridged into an Iced subscription.
//!
//! A debounced `notify` watcher runs on a dedicated OS thread (so the
//! non-`Send` debouncer never crosses an `.await`) and forwards a single
//! [`Message::FsChanged`] per debounced batch into an Iced stream. The
//! subscription's identity is the set of watched roots, so it restarts only when
//! those change.

use std::path::PathBuf;
use std::time::Duration;

use iced::Subscription;
use notify::RecursiveMode;
use notify_debouncer_full::{new_debouncer, DebounceEventResult};

use crate::app::Message;

pub fn subscription(roots: Vec<PathBuf>) -> Subscription<Message> {
    if roots.is_empty() {
        return Subscription::none();
    }
    Subscription::run_with(roots, build)
}

#[allow(clippy::ptr_arg)] // signature must match `Subscription::run_with`'s `fn(&D)`
fn build(roots: &Vec<PathBuf>) -> impl iced::futures::Stream<Item = Message> {
    let roots = roots.clone();
    iced::stream::channel(
        64,
        move |sender: iced::futures::channel::mpsc::Sender<Message>| async move {
            // Own the debouncer entirely on a std thread; it parks forever to keep
            // the watch alive while the sender keeps the stream open.
            std::thread::spawn(move || {
                let mut cb_sender = sender;
                let debouncer = new_debouncer(
                    Duration::from_millis(250),
                    None,
                    move |result: DebounceEventResult| {
                        if let Ok(events) = result {
                            if !events.is_empty() {
                                let _ = cb_sender.try_send(Message::FsChanged);
                            }
                        }
                    },
                );
                if let Ok(mut debouncer) = debouncer {
                    for root in &roots {
                        let _ = debouncer.watch(root, RecursiveMode::Recursive);
                    }
                    loop {
                        std::thread::park();
                    }
                }
            });

            std::future::pending::<()>().await;
        },
    )
}
