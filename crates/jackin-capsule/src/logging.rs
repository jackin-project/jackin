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
//!   - `JACKIN_CAPSULE_LOG_PATH` env var (when set) overrides the
//!     default. Useful for tests and for operators that want the log
//!     somewhere other than the state dir.
//!   - Default: `/jackin/state/multiplexer.log`.
//!   - If the file cannot be opened for append, the logger silently
//!     falls back to stderr-only. Logging must never block startup.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use chrono::{SecondsFormat, Utc};

static LOG_FILE: OnceLock<Mutex<Option<File>>> = OnceLock::new();
static PANIC_HOOK_INSTALLED: OnceLock<()> = OnceLock::new();
static DEBUG_ENABLED: AtomicBool = AtomicBool::new(false);

/// `true` when `JACKIN_DEBUG=1` (or any truthy value) was set in the
/// container's env. Captured once at `init()` time so per-line emit
/// paths can branch on it cheaply. Verbose `cdebug!` callers compile
/// the format args lazily and skip the file write when `false` —
/// production runs stay quiet, `--debug` runs get the firehose.
pub fn debug_enabled() -> bool {
    DEBUG_ENABLED.load(Ordering::Relaxed)
}

/// Default in-container path. The host's state-dir mount makes this
/// readable from outside the container.
const DEFAULT_LOG_PATH: &str = "/jackin/state/multiplexer.log";

/// Resolve the log path. Honours `JACKIN_CAPSULE_LOG_PATH` first,
/// falls back to the default.
fn resolve_log_path() -> PathBuf {
    std::env::var_os("JACKIN_CAPSULE_LOG_PATH")
        .map_or_else(|| PathBuf::from(DEFAULT_LOG_PATH), PathBuf::from)
}

/// Open the log file for append. Called once from the daemon entry
/// point. Failures (path not writable, dir missing) are swallowed —
/// the logger keeps emitting to stderr.
pub fn init() {
    // Honour the same env var the host CLI sets when the operator
    // launches with `--debug`. Truthy values: `1`, `true`, `yes`, `on`
    // (case-insensitive). Anything else (including unset) leaves the
    // verbose surface off.
    let debug = std::env::var("JACKIN_DEBUG").is_ok_and(|v| {
        matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    });
    DEBUG_ENABLED.store(debug, Ordering::Relaxed);

    let path = resolve_log_path();
    #[expect(
        clippy::disallowed_methods,
        reason = "capsule log opens once during logging initialization, before render loop work"
    )]
    let file = match OpenOptions::new().create(true).append(true).open(&path) {
        Ok(f) => Some(f),
        Err(e) => {
            crate::output::stderr_line(format_args!(
                "[jackin-capsule] log file open failed for {}: {e} (errno={:?})",
                path.display(),
                e.raw_os_error()
            ));
            None
        }
    };
    if let Some(mut f) = file.as_ref().and_then(|f| f.try_clone().ok()) {
        // Drop a startup marker so the operator can tell where one
        // multiplexer run ends and the next begins in a long-lived
        // log file. The marker is the only line the logger emits
        // without going through `write_line`, so spell the timestamp
        // here too — operators pasting a tail must see when the
        // session started without scrolling for the first normal line.
        let pid = std::process::id();
        let ts = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        let _unused = writeln!(
            f,
            "{ts} ---- multiplexer start pid={pid} debug={debug} path={} ----",
            path.display()
        );
    }
    drop(LOG_FILE.set(Mutex::new(file)));
    let () = PANIC_HOOK_INSTALLED.get_or_init(|| {
        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            // Capture a backtrace immediately — before the default hook runs — so
            // it appears in the run log even when RUST_BACKTRACE is not set.
            let bt = std::backtrace::Backtrace::force_capture();
            write_line(&format!("[jackin-capsule] PANIC: {info}"));
            write_line(&format!("[jackin-capsule] BACKTRACE:\n{bt}"));
            default_hook(info);
        }));
    });
}

/// Emit one line to both stderr and the log file (if open). Lines
/// are timestamped (ISO-8601 UTC, millisecond precision) and prefixed
/// with the `[jackin-capsule]` tag, so log readers see a uniform
/// format regardless of which sink they consult. The timestamp is
/// load-bearing for bug reports: operators paste a tail and the
/// sequence of events has to be reconstructable from the file alone.
pub fn write_line(message: &str) {
    let ts = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    let stamped = format!("{ts} {message}");
    crate::output::stderr_line(format_args!("{stamped}"));
    let Some(mutex) = LOG_FILE.get() else {
        return;
    };
    let Ok(mut guard) = mutex.lock() else {
        return;
    };
    let Some(file) = guard.as_mut() else {
        return;
    };
    // `File::write` is unbuffered, so `flush` is a no-op here; per-line
    // append plus the OS page cache is enough for tail-style log reading.
    drop(writeln!(file, "{stamped}"));
}

/// Convenience macro: format + tag + emit. Always emits regardless
/// of debug mode — reserved for compact production telemetry
/// (lifecycle events, action breadcrumbs, error paths).
#[macro_export]
macro_rules! clog {
    ($($arg:tt)*) => {{
        let line = format!("[jackin-capsule] {}", format_args!($($arg)*));
        $crate::logging::write_line(&line);
        $crate::telemetry::bridge_log(false, &line);
    }};
}

/// Debug-only verbose telemetry. Compiles in unconditionally but
/// skips the format + write entirely when `JACKIN_DEBUG` is unset, so
/// production runs pay nothing for the per-byte input dumps, per-frame
/// render notes, and per-event dispatch traces this macro is meant
/// for. Use for the kind of detail a triage session needs but a quiet
/// daily-driver log must not carry.
#[macro_export]
macro_rules! cdebug {
    ($($arg:tt)*) => {{
        if $crate::logging::debug_enabled() {
            let line = format!("[jackin-capsule debug] {}", format_args!($($arg)*));
            $crate::logging::write_line(&line);
            $crate::telemetry::bridge_log(true, &line);
        }
    }};
}
