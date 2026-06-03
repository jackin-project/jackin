//! Debug-mode flag, debug-output buffering, and compact-log emission.

use std::sync::{
    Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
};

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
        eprintln!("{line}");
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
        eprintln!("{line}");
    }
}

/// Emit a compact operator-visible line unless a rich surface owns the terminal.
///
/// The line is always mirrored into the active diagnostics run when one exists,
/// so suppressed rich-surface output remains recoverable.
pub fn emit_compact_line(kind: &str, line: &str) {
    if let Some(run) = crate::run::active_run() {
        run.compact(kind, line);
    }
    if !crate::terminal::rich_terminal_owned() {
        eprintln!("{line}");
    }
}

/// Format a single debug-log line. Pure (no I/O) so unit tests can
/// assert on the wire format without touching global state or stderr.
#[must_use]
pub fn format_debug_line(category: &str, message: &str) -> String {
    format!("[jackin debug {category}] {message}")
}
