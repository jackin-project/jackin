//! Re-exports terminal-ownership helpers from `jackin-tui::ownership`.
//!
//! The authoritative state lives in `jackin_tui::ownership`; this module
//! re-exports it so existing `jackin_diagnostics::*` call sites compile
//! unchanged while `jackin-diagnostics` keeps observability only.

pub use jackin_core::shorten_home;
pub use jackin_tui::ownership::{
    host_screen_owned, reassert_alt_screen, rich_surface_active, rich_terminal_owned,
    set_host_screen_owned, set_rich_surface_active, set_terminal_title,
};
