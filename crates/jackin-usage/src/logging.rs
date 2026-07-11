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

use jackin_core::container_paths;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use chrono::{SecondsFormat, Utc};

static LOG_FILE: OnceLock<Mutex<Option<File>>> = OnceLock::new();
static PANIC_HOOK_INSTALLED: OnceLock<()> = OnceLock::new();
static DEBUG_ENABLED: AtomicBool = AtomicBool::new(false);
static TRACE_ENABLED: AtomicBool = AtomicBool::new(false);

/// `true` when `JACKIN_DEBUG=1` (or any truthy value) was set in the
/// container's env. Captured once at `init()` time so per-line emit
/// paths can branch on it cheaply. Verbose `cdebug!` callers compile
/// the format args lazily and skip the file write when `false` —
/// production runs stay quiet, `--debug` runs get the firehose.
pub fn debug_enabled() -> bool {
    DEBUG_ENABLED.load(Ordering::Relaxed)
}

/// `true` when the operator explicitly requested the trace telemetry tier. Raw
/// PTY, input, and emitted-frame payloads live here instead of the debug tier.
pub fn trace_enabled() -> bool {
    TRACE_ENABLED.load(Ordering::Relaxed)
}

/// Default in-container path. The host's state-dir mount makes this
/// readable from outside the container.
const DEFAULT_LOG_PATH: &str = container_paths::MULTIPLEXER_LOG;
const MAX_LOG_BYTES: u64 = 32 * 1024 * 1024;

/// Resolve the log path. Honours `JACKIN_CAPSULE_LOG_PATH` first,
/// falls back to the default.
fn resolve_log_path() -> PathBuf {
    std::env::var_os("JACKIN_CAPSULE_LOG_PATH")
        .map_or_else(|| PathBuf::from(DEFAULT_LOG_PATH), PathBuf::from)
}

fn rotate_if_oversized(path: &PathBuf) -> std::io::Result<()> {
    if path
        .metadata()
        .is_ok_and(|metadata| metadata.len() > MAX_LOG_BYTES)
    {
        let rotated_name = path.file_name().map_or_else(
            || "multiplexer.log.1".to_owned(),
            |name| format!("{}.1", name.to_string_lossy()),
        );
        let rotated = path.with_file_name(rotated_name);
        match fs::remove_file(&rotated) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        fs::rename(path, rotated)?;
    }
    Ok(())
}

/// Open the log file for append. Called once from the daemon entry
/// point. Failures (path not writable, dir missing) are swallowed —
/// the logger keeps emitting to stderr.
pub fn init() {
    // Honour the legacy env var the host CLI sets for `--debug` plus the newer
    // telemetry-level contract. Truthy `JACKIN_DEBUG` values: `1`, `true`,
    // `yes`, `on` (case-insensitive). `JACKIN_TELEMETRY_LEVEL=debug|trace`
    // also enables the verbose local capsule log surface.
    let telemetry_level = std::env::var("JACKIN_TELEMETRY_LEVEL")
        .ok()
        .map(|value| value.trim().to_ascii_lowercase());
    let debug = std::env::var("JACKIN_DEBUG").is_ok_and(|v| {
        matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    }) || telemetry_level
        .as_deref()
        .is_some_and(|level| matches!(level, "debug" | "trace"));
    let trace = telemetry_level.as_deref() == Some("trace");
    DEBUG_ENABLED.store(debug, Ordering::Relaxed);
    TRACE_ENABLED.store(trace, Ordering::Relaxed);

    let path = resolve_log_path();
    let file = if crate::telemetry::otlp_active() {
        None
    } else {
        // Rotate only at daemon start so `tail -f` remains stable for a live session.
        if let Err(e) = rotate_if_oversized(&path) {
            crate::output::stderr_line(format_args!(
                "[jackin-capsule] log rotation failed for {}: {e} (errno={:?})",
                path.display(),
                e.raw_os_error()
            ));
        }
        #[expect(
            clippy::disallowed_methods,
            reason = "capsule log opens once during logging initialization, before render loop work"
        )]
        match OpenOptions::new().create(true).append(true).open(&path) {
            Ok(f) => Some(f),
            Err(e) => {
                crate::output::stderr_line(format_args!(
                    "[jackin-capsule] log file open failed for {}: {e} (errno={:?})",
                    path.display(),
                    e.raw_os_error()
                ));
                None
            }
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
            "{ts} ---- multiplexer start pid={pid} debug={debug} trace={trace} path={} ----",
            path.display()
        );
        // One context banner makes the file joinable offline to the host run /
        // OTLP timeline. Per-line stamping is deliberately out (volume).
        let (run_id, session_id, traceparent) = crate::telemetry::session_context().map_or_else(
            || {
                (
                    std::env::var("JACKIN_RUN_ID").unwrap_or_else(|_| "-".to_owned()),
                    "-".to_owned(),
                    std::env::var("TRACEPARENT").unwrap_or_else(|_| "-".to_owned()),
                )
            },
            |(session_id, run_id, traceparent)| {
                (
                    run_id.unwrap_or_else(|| "-".to_owned()),
                    session_id,
                    traceparent.unwrap_or_else(|| "-".to_owned()),
                )
            },
        );
        let _unused = writeln!(
            f,
            "{ts} [jackin-capsule] context run_id={run_id} session_id={session_id} traceparent={traceparent}"
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
            // The hook runs while unwinding; keep the bridged record one-line
            // and leave the backtrace in the local multiplexer log.
            crate::telemetry::bridge_log(
                crate::telemetry::BridgeLevel::Error,
                &format!("PANIC: {info}"),
            );
            crate::telemetry::shutdown();
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
        $crate::telemetry::bridge_log($crate::telemetry::BridgeLevel::Info, &line);
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
            $crate::telemetry::bridge_log($crate::telemetry::BridgeLevel::Debug, &line);
        }
    }};
}

/// Trace-only payload telemetry. When OTLP is active, raw payload records are
/// exported at TRACE and are not mirrored to stderr or `multiplexer.log`; when
/// OTLP is inactive, they fall back to the local capsule log for offline triage.
#[macro_export]
macro_rules! ctrace_payload {
    ($($arg:tt)*) => {{
        if $crate::logging::trace_enabled() {
            let line = format!("[jackin-capsule trace] {}", format_args!($($arg)*));
            if $crate::telemetry::otlp_active() {
                $crate::telemetry::bridge_log($crate::telemetry::BridgeLevel::Trace, &line);
            } else {
                $crate::logging::write_line(&line);
            }
        }
    }};
}

#[macro_export]
macro_rules! cdebug_local {
    ($($arg:tt)*) => {{
        $crate::ctrace_payload!($($arg)*);
    }};
}

#[macro_export]
macro_rules! cwarn {
    ($($arg:tt)*) => {{
        let line = format!("[jackin-capsule] {}", format_args!($($arg)*));
        $crate::logging::write_line(&line);
        $crate::telemetry::bridge_log($crate::telemetry::BridgeLevel::Warn, &line);
    }};
}

#[macro_export]
macro_rules! cerror {
    ($($arg:tt)*) => {{
        let line = format!("[jackin-capsule] {}", format_args!($($arg)*));
        $crate::logging::write_line(&line);
        $crate::telemetry::bridge_log($crate::telemetry::BridgeLevel::Error, &line);
    }};
}

#[cfg(test)]
mod tests;
