//! jackin-test-support: shared test fakes and role-repo seed fixtures.
//!
//! **Architecture Invariant:** T3.
//! Entry point: [`FakeRunner`] — canonical `CommandRunner` test fake.

pub mod docker;
pub mod fixture_http;
pub mod fixture_process;
pub mod runner;
pub mod seed;
pub mod snapshot;
pub mod time;

pub use docker::FakeDockerClient;
pub use fixture_http::{FixtureHttpServer, RecordedRequest, ScriptedResponse, redact_secrets};
pub use fixture_process::{FakeBinary, FakeProcessHarness, Invocation, ProcessScript};
pub use runner::FakeRunner;
pub use seed::{TEST_DOCKERFILE_FROM, first_temp_role_repo, seed_valid_role_repo};
pub use snapshot::{normalize_snapshot_text, redact_digit_runs};
pub use time::ManualClock;
