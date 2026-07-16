//! jackin: host CLI binary and top-level dispatch.
//!
//! **Architecture Invariant:** T6.
//! Entry point: [`main`] — host CLI binary entry.

#![expect(
    clippy::redundant_pub_crate,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
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
pub mod brand_output;

pub mod cli;
pub mod console;
pub mod terminal_ownership;

pub mod error;
mod lifecycle;
pub(crate) mod preflight;
pub mod prompt;
pub mod role_authoring;
mod role_claude_plugins;
pub mod warp;
pub mod workspace;

pub use app::run;

#[doc(hidden)]
pub use lifecycle::{
    BinaryKind, InvocationTelemetry, LifecyclePolicy, ProductLifecycle, ResultClassification,
    classify_error, classify_parse_error, lifecycle_policy,
};

#[doc(hidden)]
pub fn install_default_tls_provider() {
    match rustls::crypto::aws_lc_rs::default_provider().install_default() {
        Ok(()) | Err(_) => {}
    }
}
