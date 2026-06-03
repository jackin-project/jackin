//! Host-side TUI helpers: debug-mode flag, warp intro, and terminal-mode guards.
//!
//! Invariant: `set_rich_surface_active(true)` must be paired with
//! `set_rich_surface_active(false)` — ancillary stderr output gates on this flag
//! to avoid streaming over a full-screen ratatui cockpit.
//!
//! Not responsible for: ratatui widget rendering or capsule-side TUI code.

use std::sync::{
    Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
};

static DEBUG_MODE: AtomicBool = AtomicBool::new(false);
static DEBUG_BUFFER_ACTIVE: AtomicBool = AtomicBool::new(false);
static DEBUG_BUFFER: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
const DEBUG_BUFFER_LIMIT: usize = 2048;

pub fn set_debug_mode(enabled: bool) {
    DEBUG_MODE.store(enabled, Ordering::Relaxed);
}

/// Whether `--debug` was passed. Hot path — must stay an atomic-load.
#[must_use]
pub fn is_debug_mode() -> bool {
    DEBUG_MODE.load(Ordering::Relaxed)
}

static RICH_SURFACE_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Set while a full-screen rich TUI owns the alternate screen.
///
/// Ancillary stderr status output — spinners, "waiting" lines — checks this
/// and stays silent so it cannot stream over the cockpit. Driven by the
/// renderer's lifetime, never by callers.
pub fn set_rich_surface_active(active: bool) {
    RICH_SURFACE_ACTIVE.store(active, Ordering::Relaxed);
}

#[must_use]
pub fn rich_surface_active() -> bool {
    RICH_SURFACE_ACTIVE.load(Ordering::Relaxed)
}

static HOST_SCREEN_OWNED: AtomicBool = AtomicBool::new(false);

/// Set while a single host-side guard owns the screen for a whole launch flow.
///
/// The guard holds the alternate screen, raw mode, and mouse capture across
/// console → loading → capsule → exit. The individual surfaces (console
/// manager, launch cockpit, exit outro) check this and skip their own
/// enter/leave so the flow never drops back to the cooked terminal between
/// screens. Driven only by the owning guard's lifetime.
pub fn set_host_screen_owned(owned: bool) {
    HOST_SCREEN_OWNED.store(owned, Ordering::Relaxed);
}

#[must_use]
pub fn host_screen_owned() -> bool {
    HOST_SCREEN_OWNED.load(Ordering::Relaxed)
}

/// True when any host-side full-screen surface owns terminal modes that make
/// direct stdout/stderr streaming unsafe.
///
/// `rich_surface_active` tracks a currently drawing cockpit/dialog. The host
/// guard can outlive an individual renderer while still holding raw mode,
/// mouse capture, and the alternate screen across console → launch → capsule.
/// Plain command output is equally corrupting in that gap.
#[must_use]
pub fn rich_terminal_owned() -> bool {
    rich_surface_active() || host_screen_owned()
}

/// Re-enter the host alternate screen after an interactive child returns.
///
/// A baked capsule still drops `?1049l` on detach and returns the terminal to
/// the primary screen; re-asserting the moment the `docker exec` returns means
/// the post-attach work (outcome inspection, the exit outro) renders on the
/// alternate screen instead of flashing the operator's shell. No-op unless a
/// host guard owns the screen.
pub fn reassert_alt_screen() {
    use crossterm::ExecutableCommand as _;
    if !host_screen_owned() {
        return;
    }
    let mut out = std::io::stdout();
    let _ = out.execute(crossterm::terminal::EnterAlternateScreen);
    let _ = out.execute(crossterm::cursor::Hide);
}

/// Format a single debug-log line. Pure (no I/O) so unit tests can
/// assert on the wire format without touching global state or stderr.
#[must_use]
pub fn format_debug_line(category: &str, message: &str) -> String {
    format!("[jackin debug {category}] {message}")
}

fn debug_buffer() -> &'static Mutex<Vec<String>> {
    DEBUG_BUFFER.get_or_init(|| Mutex::new(Vec::new()))
}

fn drain_debug_buffer() -> Vec<String> {
    let mut guard = debug_buffer()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    std::mem::take(&mut *guard)
}

pub(crate) fn begin_debug_buffering() {
    DEBUG_BUFFER_ACTIVE.store(true, Ordering::Relaxed);
}

pub(crate) fn end_debug_buffering() {
    DEBUG_BUFFER_ACTIVE.store(false, Ordering::Relaxed);
    for line in drain_debug_buffer() {
        eprintln!("{line}");
    }
}

/// Drain the debug buffer and return its contents without printing to stderr.
/// Only for use in tests that need to assert on debug output.
#[cfg(test)]
pub(crate) fn drain_debug_buffer_for_test() -> Vec<String> {
    DEBUG_BUFFER_ACTIVE.store(false, Ordering::Relaxed);
    drain_debug_buffer()
}

pub fn emit_debug_line(category: &str, message: &str) {
    let line = format_debug_line(category, message);
    if crate::diagnostics::active_debug(category, &line) {
        return;
    }
    // A diagnostics run is active but not capturing (a non-`--debug` run): the
    // firehose stays off, so the line is dropped here rather than streamed to
    // the screen. Skipping this drop lets debug-tier output `eprintln!` over a
    // live rich surface (the launch cockpit owns the screen with no buffering),
    // violating the never-spew-over-a-rich-TUI rule. The buffer/stderr fallback
    // below is only for contexts with no active run (early startup, tests).
    if crate::diagnostics::active_run().is_some() {
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
    if let Some(run) = crate::diagnostics::active_run() {
        run.compact(kind, line);
    }
    if !rich_terminal_owned() {
        eprintln!("{line}");
    }
}

/// Verbose-trace helper for `--debug` runs. No-op when the flag is off
/// — formatting is deferred behind the gate so disabled call sites cost
/// only an atomic load.
///
/// `category` is a short tag (`isolation`, `worktree`, etc.) that lets
/// shared logs be greppable. Use the `format!`-style trailing args:
///
/// ```ignore
/// debug_log!("isolation", "git worktree add -b {branch} {path}");
/// ```
#[macro_export]
macro_rules! debug_log {
    ($category:expr, $($arg:tt)*) => {
        if $crate::tui::is_debug_mode() {
            $crate::tui::emit_debug_line($category, &format!($($arg)*));
        }
    };
}

// ── Shared color palette ─────────────────────────────────────────────────

const fn palette_tuple(color: jackin_tui::Rgb) -> (u8, u8, u8) {
    (color.r, color.g, color.b)
}

const WHITE: (u8, u8, u8) = palette_tuple(jackin_tui::WHITE);
const PHOSPHOR_GREEN: (u8, u8, u8) = palette_tuple(jackin_tui::PHOSPHOR_GREEN);
const PHOSPHOR_DIM: (u8, u8, u8) = palette_tuple(jackin_tui::PHOSPHOR_DIM);

const fn rgb(color: (u8, u8, u8)) -> owo_colors::Rgb {
    owo_colors::Rgb(color.0, color.1, color.2)
}

pub mod animation;
pub mod output;
pub mod prompt;

pub use animation::{warp_end_caption, warp_intro, warp_out};
pub use output::{
    clear_screen, fatal, hint, print_deploying, set_terminal_title, shorten_home, step_fail,
};
pub use prompt::{prompt_choice, require_interactive_stdin, spin_wait};

#[cfg(test)]
mod tests;
