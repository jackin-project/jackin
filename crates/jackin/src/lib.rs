//! jackin' library crate root.
//!
//! Re-exports the public module tree consumed by `main.rs`, `src/bin/role.rs`,
//! and integration tests. The crate is simultaneously a binary (via `main.rs`)
//! and a library (via `lib.rs`); `pub mod` here is the library boundary.

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
pub mod agent_binary;
pub mod ansi_text;
pub mod app;
pub mod binary_artifact;
pub mod capsule_binary;
pub mod cli;
pub mod config;
pub mod console;
pub mod derived_image;
pub mod diagnostics;
pub mod docker;
pub mod docker_client;
pub mod env_model;
pub mod env_resolver;
pub mod error;
pub mod host_claude;
pub mod instance;
pub mod isolation;
pub mod manifest;
pub mod net;
pub mod operator_env;
pub mod paths;
pub mod preflight;
pub mod repo;
pub mod repo_contract;
pub mod role_authoring;
pub mod runtime;
pub mod selector;
pub mod tui;
pub mod version_check;
pub mod workspace;

pub use app::run;
