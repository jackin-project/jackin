// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Stderr and governed OTLP logging for the in-container multiplexer.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::{SecondsFormat, Utc};

static PANIC_HOOK_INSTALLED: OnceLock<()> = OnceLock::new();
static DEBUG_ENABLED: AtomicBool = AtomicBool::new(false);
static TRACE_ENABLED: AtomicBool = AtomicBool::new(false);

/// `true` when the effective capsule telemetry level is debug or trace.
/// Captured once at `init()` time so per-line emit
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

pub fn init() {
    // One shared resolver owns JACKIN_TELEMETRY_LEVEL and config precedence.
    let level = jackin_diagnostics::telemetry_level(false);
    let debug = matches!(
        level,
        jackin_diagnostics::TelemetryLevel::Debug | jackin_diagnostics::TelemetryLevel::Trace
    );
    let trace = matches!(level, jackin_diagnostics::TelemetryLevel::Trace);
    DEBUG_ENABLED.store(debug, Ordering::Relaxed);
    TRACE_ENABLED.store(trace, Ordering::Relaxed);

    let () = PANIC_HOOK_INSTALLED.get_or_init(|| {
        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            // Capture a backtrace immediately — before the default hook runs — so
            // it appears in the run log even when RUST_BACKTRACE is not set.
            let bt = std::backtrace::Backtrace::force_capture();
            write_line(&format!("[jackin-capsule] PANIC: {info}"));
            write_line(&format!("[jackin-capsule] BACKTRACE:\n{bt}"));
            let _ = jackin_telemetry::emit_event(
                &jackin_telemetry::event::APP_CRASH,
                jackin_telemetry::FieldSet::new(&[], Some(&format!("PANIC: {info}"))),
            );
            crate::telemetry::bridge_log(
                crate::telemetry::BridgeLevel::Error,
                &format!("PANIC: {info}"),
            );
            crate::telemetry::shutdown();
            default_hook(info);
        }));
    });
}

/// Emit one timestamped line to stderr for Docker's process log stream.
pub fn write_line(message: &str) {
    let ts = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    let stamped = format!("{ts} {message}");
    crate::output::stderr_line(format_args!("{stamped}"));
}

/// Convenience macro: format + tag + emit. Always emits regardless
/// of debug mode — reserved for compact production telemetry
/// (lifecycle events, action breadcrumbs, error paths).
///
/// The raw body is built once; `[jackin-capsule]` is applied only on the
/// file/stderr render path. OTLP receives the prefix-free body with schema attrs.
#[macro_export]
macro_rules! clog {
    ($($arg:tt)*) => {{
        let body = format!("{}", format_args!($($arg)*));
        $crate::logging::write_line(&format!("[jackin-capsule] {body}"));
        $crate::telemetry::bridge_log($crate::telemetry::BridgeLevel::Info, &body);
    }};
}

/// Debug-only verbose telemetry. Compiles in unconditionally but
/// skips the format + write entirely below the debug telemetry level, so
/// production runs pay nothing for the per-byte input dumps, per-frame
/// render notes, and per-event dispatch traces this macro is meant
/// for. Use for the kind of detail a triage session needs but a quiet
/// daily-driver log must not carry.
#[macro_export]
macro_rules! cdebug {
    ($($arg:tt)*) => {{
        if $crate::logging::debug_enabled() {
            let body = format!("{}", format_args!($($arg)*));
            $crate::logging::write_line(&format!("[jackin-capsule debug] {body}"));
            $crate::telemetry::bridge_log($crate::telemetry::BridgeLevel::Debug, &body);
        }
    }};
}

/// Trace-only payload telemetry exported through the governed OTLP facade.
#[macro_export]
macro_rules! ctrace_payload {
    ($($arg:tt)*) => {{
        if $crate::logging::trace_enabled() {
            let body = format!("{}", format_args!($($arg)*));
            $crate::telemetry::bridge_log($crate::telemetry::BridgeLevel::Trace, &body);
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
        let body = format!("{}", format_args!($($arg)*));
        $crate::logging::write_line(&format!("[jackin-capsule] {body}"));
        $crate::telemetry::bridge_log($crate::telemetry::BridgeLevel::Warn, &body);
    }};
}

#[macro_export]
macro_rules! cerror {
    ($($arg:tt)*) => {{
        let body = format!("{}", format_args!($($arg)*));
        $crate::logging::write_line(&format!("[jackin-capsule] {body}"));
        $crate::telemetry::bridge_log($crate::telemetry::BridgeLevel::Error, &body);
    }};
}
