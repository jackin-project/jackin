//! Re-export of `jackin_tui::ownership` for backward compatibility.
//!
//! The terminal-ownership flags and accessors were moved from this crate
//! to `jackin_tui` in A4 (P7 — keep the L1 domain diagnostic substrate
//! free of presentation state). This shim preserves every existing
//! `jackin_diagnostics::terminal::*` call site unchanged. New code
//! should reach for `jackin_tui::ownership::*` directly.

pub use jackin_tui::ownership::{
    host_screen_owned, reassert_alt_screen, rich_surface_active, rich_terminal_owned,
    set_host_screen_owned, set_rich_surface_active, set_terminal_title,
};
