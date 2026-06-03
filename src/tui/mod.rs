//! Host-side TUI helpers: debug-mode flag, warp intro, and terminal-mode guards.
//!
//! Invariant: `set_rich_surface_active(true)` must be paired with
//! `set_rich_surface_active(false)` — ancillary stderr output gates on this flag
//! to avoid streaming over a full-screen ratatui cockpit.
//!
//! Not responsible for: ratatui widget rendering or capsule-side TUI code.

// ── Logging and terminal substrate — re-exported from jackin-diagnostics ───

pub use jackin_diagnostics::{
    emit_compact_line, emit_debug_line, format_debug_line, is_debug_mode, set_debug_mode,
    host_screen_owned, reassert_alt_screen, rich_surface_active, rich_terminal_owned,
    set_host_screen_owned, set_rich_surface_active, set_terminal_title, shorten_home,
};
pub(crate) use jackin_diagnostics::{begin_debug_buffering, end_debug_buffering};

#[cfg(test)]
pub(crate) use jackin_diagnostics::drain_debug_buffer_for_test;

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
pub use output::{clear_screen, fatal, hint, print_deploying, step_fail};
pub use prompt::{prompt_choice, require_interactive_stdin, spin_wait};

// Thin macro wrapper so existing `debug_log!(...)` call sites in the binary
// continue to work without per-file imports.
#[doc(hidden)]
#[macro_export]
macro_rules! debug_log {
    ($($t:tt)*) => { ::jackin_diagnostics::debug_log!($($t)*) }
}
