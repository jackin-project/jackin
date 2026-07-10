//! Canonical fakes and role-repo seed helpers shared across jackin❯ workspace
//! test suites.
//!
//! Not responsible for: asserting test outcomes — callers inspect
//! `FakeRunner::recorded` / `FakeDockerClient::recorded` and friends directly
//! after the call under test. Production crates must never depend on this
//! crate; it is consumed via `[dev-dependencies]` only.

pub mod docker;
pub mod runner;
pub mod seed;

pub use docker::FakeDockerClient;
pub use runner::FakeRunner;
pub use seed::{TEST_DOCKERFILE_FROM, first_temp_role_repo, seed_valid_role_repo};
