/// Library target so integration tests under `tests/` can exercise
/// the protocol, prefix-key parser, VT round-trips, and status bar
/// without spawning a PTY.
pub mod attach_protocol;
pub mod action;
pub mod branch_context_bar;
pub mod client;
pub mod config;
pub mod daemon;
pub mod dialog;
pub mod git_context;
pub mod input;
pub mod layout;
pub mod logging;
pub mod mouse_protocol;
pub mod mux_mode;
pub mod pid1;
pub mod pr_context;
pub mod protocol;
pub mod render;
pub mod runtime_setup;
pub mod session;
pub mod pane_widget;
pub mod selection;
pub mod socket;

pub mod socket_backend;
pub mod statusbar;
pub mod title;
pub mod terminal_geometry;
pub mod util;
