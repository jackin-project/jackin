// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Capsule crate root: module declarations re-exported for the in-container
//! daemon and host client binaries.
//!
//! Not responsible for: protocol encoding (see `jackin-protocol`), host-side
//! launch orchestration, or config schema migration.
//!
//! **Architecture Invariant:** L4 entry/glue crate. Allowed dependencies:
//! `jackin-core`, `jackin-diagnostics`, `jackin-protocol`, `jackin-usage`,
//! `jackin-term`, `jackin-tui`. PID1 + PTY daemon + in-container TUI +
//! usage telemetry re-exported from `jackin-usage` (see C2 carve).
//! Must NOT depend on host-side runtime (`jackin-runtime`) or other
//! host binary crates — the capsule is a different process tree.

pub mod agent_status;
pub(crate) mod alloc_telemetry;
pub mod attach_context;
/// Library target so integration tests under `tests/` can exercise
/// the protocol, prefix-key parser, VT round-trips, and status bar
/// without spawning a PTY.
pub mod attach_protocol;
pub mod client;
pub(crate) mod client_writer;
pub(crate) mod clipboard;
pub mod config;
pub mod container_context;
pub mod daemon;
pub(crate) mod debug_panic;
pub mod exec;
pub mod exit_assess;
pub mod firewall;
pub mod git_context;
pub mod mcp_server;
pub mod output;
pub mod pid1;
pub mod pr_context;
pub mod protocol;
pub mod pull_request;
pub mod runtime_setup;
pub mod services;
pub mod session;
pub mod socket;
pub mod sudo_provision;
pub mod util;

/// Terminal-rendering code — all UI paint/layout lives here.
pub mod tui;
pub mod wordlist;

// Logging infrastructure lives in jackin-usage; re-export so all
// capsule modules that call crate::clog! / crate::cdebug! still work —
// $crate in the macro expands to jackin_usage, which has write_line.
pub mod logging {
    pub use jackin_usage::logging::*;
}
pub use jackin_usage::{cdebug, clog};
pub mod telemetry {
    pub use jackin_usage::telemetry::*;
}
pub mod token_monitor {
    pub use jackin_usage::token_monitor::*;
}
pub mod usage {
    pub use jackin_usage::usage::*;
}
