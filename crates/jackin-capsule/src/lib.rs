/// Library target so integration tests under `tests/` can exercise
/// the protocol, prefix-key parser, VT round-trips, and status bar
/// without spawning a PTY.
pub mod attach_protocol;
pub mod attach_context;
pub mod client;
pub mod config;
pub mod container_context;
pub mod daemon;
pub mod git_context;
pub mod logging;
pub mod mouse_protocol;
pub mod pid1;
pub mod pr_context;
pub mod protocol;
pub mod pull_request;
pub mod runtime_setup;
pub mod session;
pub mod services;
pub mod socket;
pub mod title;
pub mod util;

/// Terminal-rendering code — all UI paint/layout lives here.
pub mod tui;
