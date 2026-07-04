// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Debug-mode flag, debug-output buffering, and compact-log emission.

use std::sync::{
    Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
};
use std::{fmt::Arguments, io::Write as _};

pub(crate) static DEBUG_BUFFER_ACTIVE: AtomicBool = AtomicBool::new(false);
static DEBUG_BUFFER: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
const DEBUG_BUFFER_LIMIT: usize = 2048;

static DEBUG_MODE: AtomicBool = AtomicBool::new(false);

pub fn set_debug_mode(enabled: bool) {
    DEBUG_MODE.store(enabled, Ordering::Relaxed);
}

/// Whether `--debug` was passed. Hot path — must stay an atomic-load.
#[must_use]
pub fn is_debug_mode() -> bool {
    DEBUG_MODE.load(Ordering::Relaxed)
}

fn debug_buffer() -> &'static Mutex<Vec<String>> {
    DEBUG_BUFFER.get_or_init(|| Mutex::new(Vec::new()))
}

fn stderr_line(args: Arguments<'_>) {
    let mut stderr = std::io::stderr().lock();
    drop(writeln!(stderr, "{args}"));
}

pub(crate) fn should_tee_debug_to_stderr() -> bool {
    !DEBUG_BUFFER_ACTIVE.load(Ordering::Relaxed) && !crate::terminal::rich_terminal_owned()
}

pub(crate) fn drain_debug_buffer() -> Vec<String> {
    let mut guard = debug_buffer()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    std::mem::take(&mut *guard)
}

pub fn begin_debug_buffering() {
    DEBUG_BUFFER_ACTIVE.store(true, Ordering::Relaxed);
}

pub fn end_debug_buffering() {
    DEBUG_BUFFER_ACTIVE.store(false, Ordering::Relaxed);
    for line in drain_debug_buffer() {
        stderr_line(format_args!("{line}"));
    }
}

/// Drain the debug buffer and return its contents without printing to stderr.
/// For use in tests that need to assert on debug output.
pub fn drain_debug_buffer_for_test() -> Vec<String> {
    DEBUG_BUFFER_ACTIVE.store(false, Ordering::Relaxed);
    drain_debug_buffer()
}

pub fn emit_debug_line(category: &str, message: &str) {
    let line = format_debug_line(category, message);
    if crate::run::active_debug(category, &line) {
        if should_tee_debug_to_stderr() {
            stderr_line(format_args!("{line}"));
        }
        return;
    }
    // A diagnostics run is active but not capturing (a non-`--debug` run): the
    // firehose stays off, so the line is dropped here rather than streamed to
    // the screen. Skipping this drop lets debug-tier output `eprintln!` over a
    // live rich surface (the launch cockpit owns the screen with no buffering),
    // violating the never-spew-over-a-rich-TUI rule. The buffer/stderr fallback
    // below is only for contexts with no active run (early startup, tests).
    if crate::run::active_run().is_some() {
        return;
    }
    if DEBUG_BUFFER_ACTIVE.load(Ordering::Relaxed) {
        let mut guard = debug_buffer()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if guard.len() >= DEBUG_BUFFER_LIMIT {
            let keep_from = guard.len() / 2;
            guard.drain(..keep_from);
        }
        guard.push(line);
    } else {
        stderr_line(format_args!("{line}"));
    }
}

/// Emit a compact operator-visible line.
///
/// Always mirrored into the active diagnostics run when one exists. For the
/// terminal: printed to stderr immediately on a plain CLI; when a rich surface
/// owns the screen it is *deferred* into the debug buffer and flushed to stderr
/// at teardown ([`end_debug_buffering`]) rather than dropped — so an operator
/// notice (e.g. "OTLP export failing") still reaches the operator and any parent
/// process wrapping the command, without ever spewing over the live TUI. This
/// makes failure visibility independent of the (optional) run file.
pub fn emit_compact_line(kind: &str, line: &str) {
    if let Some(run) = crate::run::active_run() {
        run.compact(kind, line);
    }
    emit_operator_notice(line);
}

/// The terminal half of [`emit_compact_line`] with no run-file write: stderr on
/// a plain CLI, deferred to teardown under a rich surface. Use this from inside
/// the tracing layer (where emitting a `tracing` event would re-enter the
/// subscriber) — the caller writes the run file directly.
pub fn emit_operator_notice(line: &str) {
    if crate::terminal::rich_terminal_owned() {
        buffer_pending_notice(line);
    } else {
        stderr_line(format_args!("{line}"));
    }
}

/// Emit an operator notice directly to stderr, bypassing the rich-surface
/// deferral buffer. For use at final teardown only (e.g. the OTLP flush-failure
/// notice from `ActiveRunGuard::drop`): the run guard can outlive the terminal
/// session, so its buffer may already be drained — buffering here would lose the
/// notice. At process exit, writing straight to stderr cannot corrupt a live TUI
/// because the surface is already torn down.
pub fn emit_teardown_notice(line: &str) {
    stderr_line(format_args!("{line}"));
}

/// Queue an operator notice for the deferred stderr flush at rich-surface
/// teardown. Shares the debug buffer (and its cap) so it can never grow without
/// bound while a long rich session owns the screen.
fn buffer_pending_notice(line: &str) {
    let mut guard = debug_buffer()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if guard.len() >= DEBUG_BUFFER_LIMIT {
        let keep_from = guard.len() / 2;
        guard.drain(..keep_from);
    }
    guard.push(line.to_owned());
}

/// Format a single debug-log line. Pure (no I/O) so unit tests can
/// assert on the wire format without touching global state or stderr.
#[must_use]
pub fn format_debug_line(category: &str, message: &str) -> String {
    format!("[jackin debug {category}] {message}")
}
