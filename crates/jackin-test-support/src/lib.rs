//! jackin-test-support: shared test fakes and role-repo seed fixtures.
//!
//! **Architecture Invariant:** T3.
//! Entry point: [`FakeRunner`] — canonical `CommandRunner` test fake.

pub mod docker;
pub mod runner;
pub mod seed;

pub use docker::FakeDockerClient;
pub use runner::FakeRunner;
pub use seed::{TEST_DOCKERFILE_FROM, first_temp_role_repo, seed_valid_role_repo};
