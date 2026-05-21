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
        // log file.
        let pid = std::process::id();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let _ = writeln!(
            f,
            "---- multiplexer start pid={pid} epoch={now} path={} ----",
            path.display()
        );
    }
    let _ = LOG_FILE.set(Mutex::new(file));
}

/// Emit one line to both stderr and the log file (if open). Lines
/// are prefixed with the same `[jackin-container]` tag the existing
/// `eprintln!` callers already use, so log readers see a uniform
/// format regardless of which sink they consult.
pub fn write_line(message: &str) {
    eprintln!("{message}");
    let Some(mutex) = LOG_FILE.get() else {
        return;
    };
    let Ok(mut guard) = mutex.lock() else {
        return;
    };
    let Some(file) = guard.as_mut() else {
        return;
    };
    let _ = writeln!(file, "{message}");
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
