//! Shared `AppConfig` constructors for tests across the workspace.
//!
//! Lives behind the `test-support` feature so production binaries don't pull
//! the helpers in; consumers add `features = ["test-support"]` to their
//! `jackin-config` dependency only from `dev-dependencies` or from their own
//! `test-support` feature (mirrors the pattern in
//! `jackin-runtime/src/runtime/test_support.rs`).

use crate::AppConfig;
use crate::schema::RoleSource;

/// Build an `AppConfig` with one default-role row per name. Mirrors the
/// pre-extraction helper that was duplicated across three test files.
#[cfg(any(test, feature = "test-support"))]
pub fn config_with_agents(names: &[&str]) -> AppConfig {
    let mut config = AppConfig::default();
    for name in names {
        config.roles.insert((*name).into(), RoleSource::default());
    }
    config
}