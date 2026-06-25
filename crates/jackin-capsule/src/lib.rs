//! Capsule crate root: module declarations re-exported for the in-container
//! daemon and host client binaries.
//!
//! Not responsible for: protocol encoding (see `jackin-protocol`), host-side
//! launch orchestration, or config schema migration.

pub(crate) mod alloc_telemetry;
pub mod attach_context;
/// Library target so integration tests under `tests/` can exercise
/// the protocol, prefix-key parser, VT round-trips, and status bar
/// without spawning a PTY.
pub mod attach_protocol;
pub mod client;
pub(crate) mod client_writer;
pub mod config;
pub mod container_context;
pub mod daemon;
pub(crate) mod debug_panic;
pub mod exit_assess;
pub mod git_context;
pub mod logging;
pub mod output;
pub mod pid1;
pub mod pr_context;
pub mod protocol;
pub mod pull_request;
pub mod runtime_setup;
pub mod services;
pub mod session;
pub mod socket;
pub mod telemetry;
pub mod util;

/// Terminal-rendering code — all UI paint/layout lives here.
pub mod tui;
pub mod wordlist;
