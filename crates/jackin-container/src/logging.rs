//! File + stderr logging for the in-container multiplexer.
//!
//! Two destinations on every line:
//!   1. `stderr` — captured by Docker, visible via `docker logs <container>`.
//!   2. A log file under the mounted state directory (`/jackin/state/`
//!      inside the container, which the host mounts read-write under
//!      `paths.data_dir.join(<container_base>)`). The operator can
//!      `tail -f` the file from their host without entering the
//!      container or watching `docker logs`, and paste the contents
//!      when reporting bugs.
//!
//! Path resolution:
//!   - `JACKIN_CONTAINER_LOG_PATH` env var (when set) overrides the
//!     default. Useful for tests and for operators that want the log
//!     somewhere other than the state dir.
//!   - Default: `/jackin/state/multiplexer.log`.
//!   - If the file cannot be opened for append, the logger silently
//!     falls back to stderr-only. Logging must never block startup.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use chrono::{SecondsFormat, Utc};

static LOG_FILE: OnceLock<Mutex<Option<File>>> = OnceLock::new();

/// Default in-container path. The host's state-dir mount makes this
/// readable from outside the container.
const DEFAULT_LOG_PATH: &str = "/jackin/state/multiplexer.log";

/// Resolve the log path. Honours `JACKIN_CONTAINER_LOG_PATH` first,
/// falls back to the default.
fn resolve_log_path() -> PathBuf {
    std::env::var_os("JACKIN_CONTAINER_LOG_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_LOG_PATH))
}

/// Open the log file for append. Called once from the daemon entry
/// point. Failures (path not writable, dir missing) are swallowed —
/// the logger keeps emitting to stderr.
pub fn init() {
    let path = resolve_log_path();
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .ok();
    if let Some(mut f) = file.as_ref().and_then(|f| f.try_clone().ok()) {
        // Drop a startup marker so the operator can tell where one
        // multiplexer run ends and the next begins in a long-lived
        // log file. The marker is the only line the logger emits
        // without going through `write_line`, so spell the timestamp
        // here too — operators pasting a tail must see when the
        // session started without scrolling for the first normal line.
        let pid = std::process::id();
        let ts = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        let _ = writeln!(
            f,
            "{ts} ---- multiplexer start pid={pid} path={} ----",
            path.display()
        );
    }
    let _ = LOG_FILE.set(Mutex::new(file));
}

/// Emit one line to both stderr and the log file (if open). Lines
/// are timestamped (ISO-8601 UTC, millisecond precision) and prefixed
/// with the `[jackin-container]` tag, so log readers see a uniform
/// format regardless of which sink they consult. The timestamp is
/// load-bearing for bug reports: operators paste a tail and the
/// sequence of events has to be reconstructable from the file alone.
pub fn write_line(message: &str) {
    let ts = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    let stamped = format!("{ts} {message}");
    eprintln!("{stamped}");
    let Some(mutex) = LOG_FILE.get() else {
        return;
    };
    let Ok(mut guard) = mutex.lock() else {
        return;
    };
    let Some(file) = guard.as_mut() else {
        return;
    };
    let _ = writeln!(file, "{stamped}");
    let _ = file.flush();
}

/// Convenience macro: format + tag + emit. Replaces the existing
/// `eprintln!("[jackin-container] …")` pattern.
#[macro_export]
macro_rules! clog {
    ($($arg:tt)*) => {{
        let line = format!("[jackin-container] {}", format_args!($($arg)*));
        $crate::logging::write_line(&line);
    }};
}
