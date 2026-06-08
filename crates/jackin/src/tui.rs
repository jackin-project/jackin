//! Host-side TUI helpers: debug-mode flag, warp intro, and terminal-mode guards.
//!
//! Invariant: `set_rich_surface_active(true)` must be paired with
//! `set_rich_surface_active(false)` — ancillary stderr output gates on this flag
//! to avoid streaming over a full-screen ratatui cockpit.
//!
//! Not responsible for: ratatui widget rendering or capsule-side TUI code.

// ── Logging and terminal substrate — re-exported from jackin-diagnostics ───

pub(crate) use jackin_diagnostics::{begin_debug_buffering, end_debug_buffering};
pub use jackin_diagnostics::{
    emit_compact_line, emit_debug_line, format_debug_line, host_screen_owned, is_debug_mode,
    reassert_alt_screen, rich_surface_active, rich_terminal_owned, set_debug_mode,
    set_host_screen_owned, set_rich_surface_active, set_terminal_title, shorten_home,
};

// ── Output and animation — re-exported from jackin-tui ──────────────────

pub use jackin_tui::output::{clear_screen, fatal, hint, print_deploying, step_fail};

/// Entry ritual — re-exported from jackin-tui, with `host_screen_owned` resolved here.
pub fn warp_intro() {
    jackin_tui::animation::warp_intro(host_screen_owned());
}

/// Exit ritual — re-exported from jackin-tui, with `host_screen_owned` resolved here.
pub fn warp_out() {
    jackin_tui::animation::warp_out(host_screen_owned());
}

/// Closing screen — re-exported from jackin-tui, with `host_screen_owned` resolved here.
pub fn warp_end_caption(elapsed: Option<std::time::Duration>) {
    jackin_tui::animation::warp_end_caption(elapsed, host_screen_owned());
}

pub mod prompt;

pub use prompt::{prompt_choice, require_interactive_stdin};

// Thin macro wrapper so existing `debug_log!(...)` call sites in the binary
// continue to work without per-file imports.
#[doc(hidden)]
#[macro_export]
macro_rules! debug_log {
    ($($t:tt)*) => { ::jackin_diagnostics::debug_log!($($t)*) }
}
