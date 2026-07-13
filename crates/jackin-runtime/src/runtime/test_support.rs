//! Test helper: pre-install binary test stubs so `load_role` calls in
//! `jackin-runtime`'s own tests never fall through to network downloads.
//!
//! `FakeRunner`/`FakeDockerClient`/role-repo seed fixtures moved to
//! `jackin-test-support` (plan 025) — this module keeps only what stays
//! runtime-coupled (`jackin_image` binary-stub installation) and has no
//! consumer outside this crate's own test suites.
//!
//! No `clippy::expect_used` suppression needed here: this module is
//! `#[cfg(test)]`-only, and workspace `clippy.toml` sets
//! `allow-expect-in-tests = true`.

/// Pre-install all binary test stubs (agent binaries + jackin-capsule) so that
/// `load_role` calls in tests never fall through to network downloads regardless
/// of how the `cfg!(test)` flag is resolved in each dependency compilation unit.
///
/// Call this once per test that calls `load_role` or any function that internally
/// invokes `ensure_available`.
pub fn install_all_test_stubs(paths: &jackin_core::paths::JackinPaths) {
    use jackin_core::agent::Agent;
    for agent in &[
        Agent::Claude,
        Agent::Codex,
        Agent::Amp,
        Agent::Kimi,
        Agent::Opencode,
    ] {
        jackin_image::agent_binary::install_test_stub(paths, *agent)
            .expect("install agent binary test stub");
    }
    jackin_image::capsule_binary::install_test_stub(paths).expect("install capsule test stub");
}
