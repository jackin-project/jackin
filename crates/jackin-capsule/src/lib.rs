/// Library target so integration tests under `tests/` can exercise
/// the protocol, prefix-key parser, VT round-trips, and status bar
/// without spawning a PTY.
pub mod attach_protocol;
pub mod client;
pub mod config;
pub mod daemon;
pub mod git_context;
pub mod layout;
pub mod logging;
pub mod mouse_protocol;
pub mod mux_mode;
pub mod pid1;
pub mod pr_context;
pub mod protocol;
pub mod runtime_setup;
pub mod session;
pub mod socket;
pub mod terminal_geometry;
pub mod title;
pub mod util;

/// Terminal-rendering code — all UI paint/layout lives here.
pub mod tui;

// Flat re-exports so existing `crate::dialog`, `crate::statusbar`, etc.
// paths continue to resolve without changing every call site.
pub use tui::components::branch_context_bar;
pub use tui::components::status_bar as statusbar;
pub use tui::dialog;
pub use tui::input;
pub use tui::message as action;
pub use tui::render;
pub use tui::selection;
pub use tui::socket_backend;
