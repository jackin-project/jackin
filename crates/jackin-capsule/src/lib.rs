/// In-container Capsule library used by the `jackin-capsule`
/// binary. Splitting the modules into a library target lets the
/// `tests/` directory write integration tests against the protocol,
/// the prefix-key parser, the VT round-trips, and the status bar
/// without spawning a real PTY.
pub mod client;
pub mod daemon;
pub mod dialog;
pub mod input;
pub mod layout;
pub mod logging;
pub mod pid1;
pub mod protocol;
pub mod render;
pub mod runtime_setup;
pub mod session;
pub mod socket;
pub mod statusbar;
pub mod terminal_geometry;
