//! Minimal diagnostics logger so the GUI leaves a trace to debug from.
//!
//! A Finder/dock-launched app has no console, so problems (a blank terminal, a
//! wgpu/font warning, a failed spawn) otherwise vanish. This writes log records
//! to **both** a `filament.log` file in the OS data dir and stderr, so they're
//! available whether the app was double-clicked or run from a terminal.
//!
//! Verbosity: our own crate logs at `info` and dependencies at `warn` by
//! default; set `RUST_LOG=debug` (or `trace`) to raise both (handy for chasing
//! rendering/wgpu issues). No external logging crate is pulled in — just `log`.

use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use log::{LevelFilter, Metadata, Record};

struct Logger {
    file: Option<Mutex<File>>,
    /// Threshold for records from the `filament*` crates.
    app: LevelFilter,
    /// Threshold for everything else (iced, wgpu, glyphon, …).
    deps: LevelFilter,
}

impl log::Log for Logger {
    fn enabled(&self, meta: &Metadata) -> bool {
        let threshold = if meta.target().starts_with("filament") {
            self.app
        } else {
            self.deps
        };
        meta.level() <= threshold
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format!(
            "[{:<5}] {}: {}\n",
            record.level(),
            record.target(),
            record.args()
        );
        eprint!("{line}");
        if let Some(file) = &self.file {
            if let Ok(mut f) = file.lock() {
                let _ = f.write_all(line.as_bytes());
                let _ = f.flush();
            }
        }
    }

    fn flush(&self) {
        if let Some(file) = &self.file {
            if let Ok(mut f) = file.lock() {
                let _ = f.flush();
            }
        }
    }
}

/// The log file location in the OS data dir.
pub fn log_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("dev", "filament", "filament")
        .map(|d| d.data_local_dir().join("filament.log"))
}

/// Install the logger. Returns the log file path (if one was opened). Safe to
/// call once at startup; errors are swallowed so logging never breaks the app.
pub fn init() -> Option<PathBuf> {
    // `RUST_LOG` as a bare level (e.g. `debug`) raises both thresholds; otherwise
    // app=info, deps=warn.
    let override_level = std::env::var("RUST_LOG").ok().and_then(parse_level);
    let app = override_level.unwrap_or(LevelFilter::Info);
    let deps = override_level.unwrap_or(LevelFilter::Warn);

    let path = log_path();
    let file = path.as_ref().and_then(|p| {
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        File::create(p).ok().map(Mutex::new)
    });

    let logger = Logger { file, app, deps };
    // Don't compile anything out; the logger itself filters per target.
    log::set_max_level(LevelFilter::Trace);
    if log::set_boxed_logger(Box::new(logger)).is_err() {
        return None;
    }
    path
}

fn parse_level(raw: String) -> Option<LevelFilter> {
    // Accept a bare level, optionally as the last `key=value` / comma segment.
    let tok = raw
        .split([',', '='])
        .next_back()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    match tok.as_str() {
        "off" => Some(LevelFilter::Off),
        "error" => Some(LevelFilter::Error),
        "warn" => Some(LevelFilter::Warn),
        "info" => Some(LevelFilter::Info),
        "debug" => Some(LevelFilter::Debug),
        "trace" => Some(LevelFilter::Trace),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_levels() {
        assert_eq!(parse_level("debug".into()), Some(LevelFilter::Debug));
        assert_eq!(
            parse_level("wgpu=warn,info".into()),
            Some(LevelFilter::Info)
        );
        assert_eq!(parse_level("nonsense".into()), None);
    }
}
