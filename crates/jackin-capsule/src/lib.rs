/// Library target so integration tests under `tests/` can exercise
/// the protocol, prefix-key parser, VT round-trips, and status bar
/// without spawning a PTY.
pub mod client;
pub mod config;
pub mod daemon;
pub mod dialog;
pub mod exec;
pub mod input;
pub mod layout;
pub mod logging;
pub mod mcp_server;
pub mod pid1;
pub mod protocol;
pub mod render;
pub mod runtime_setup;
pub mod session;
pub mod socket;
pub mod statusbar;
pub mod terminal_geometry;
pub mod util;
pub mod wordlist;
