//! jackin❯ library crate root.
//!
//! Re-exports the module tree consumed by `main.rs`, `src/bin/role.rs`, and
//! integration tests. The crate is simultaneously a binary (via `main.rs`) and
//! a library (via `lib.rs`); `pub mod` entries here are the public compatibility
//! boundary, while `pub(crate)` entries are root-crate-only shims.

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

pub mod agent;

mod app;


pub mod cli;
pub mod config;
pub mod console;

pub(crate) mod diagnostics;
pub mod docker;
pub mod docker_client;
pub mod env_resolver;
pub mod error;
pub mod instance;
pub mod isolation;
pub mod manifest;
pub mod operator_env;
pub mod paths;
pub(crate) mod preflight;
pub mod repo;
pub(crate) mod repo_contract;
pub mod role_authoring;
pub mod runtime;
pub mod selector;
pub mod tui;
pub mod workspace;

pub use app::run;

#[doc(hidden)]
pub fn install_default_tls_provider() {
    match rustls::crypto::aws_lc_rs::default_provider().install_default() {
        Ok(()) | Err(_) => {}
    }
}
