// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! jackin❯ library crate root.
//!
//! Re-exports the module tree consumed by `main.rs`, `src/bin/role.rs`, and
//! integration tests. The crate is simultaneously a binary (via `main.rs`) and
//! a library (via `lib.rs`); `pub mod` entries here are the public compatibility
//! boundary, while `pub(crate)` entries are root-crate-only shims.
//!
//! **Architecture Invariant:** L4 entry/glue crate. Allowed dependencies:
//! `jackin-core`, `jackin-config`, `jackin-manifest`, `jackin-docker`,
//! `jackin-diagnostics`, `jackin-env`, `jackin-image`, `jackin-runtime`,
//! `jackin-tui`, `jackin-console`, `jackin-protocol`. This is the only
//! crate allowed to depend on every other workspace crate (it wires the
//! `ConsoleHostTerminal` impl, the `BuildLogSink` adapter, and the
//! runtime command dispatch). The shim-maze pattern (D6) is gone;
//! callers import directly from the owning crate root.

#![allow(clippy::redundant_pub_crate)]
#![expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "primary CLI crate owns command output rendering until output helpers are factored"
)]
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "primary CLI crate still carries state-machine invariants under the strict lint transition"
)]

mod app;

pub mod cli;
pub mod console;

pub mod error;
pub(crate) mod preflight;
pub mod prompt;
pub mod role_authoring;
mod role_claude_plugins;
pub mod warp;
pub mod workspace;

pub use app::run;

#[doc(hidden)]
pub fn install_default_tls_provider() {
    match rustls::crypto::aws_lc_rs::default_provider().install_default() {
        Ok(()) | Err(_) => {}
    }
}
