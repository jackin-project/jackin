#![expect(
    unreachable_pub,
    reason = "shared integration-test helper module: pub organizes fixtures, not a crate export"
)]

//! Shared test helpers for launch integration tests.
// pub items in a private test-helper module are intentionally unreachable
// from outside this crate; they exist for code organisation, not export.

use jackin_core::JackinPaths;
use jackin_docker::docker_client::ContainerSpec;

/// Canonical `CommandRunner` fake (shared with host/runtime suites).
pub use jackin_test_support::{FakeDockerClient, FakeRunner};

// Keep the re-export live for integration crates that do not import FakeRunner
// themselves (`per_mount_isolation_e2e` etc.) — otherwise `-D unused-imports`
// fails those targets.
fn fake_runner_usage_marker() -> FakeRunner {
    FakeRunner::for_load_agent([String::new()])
}
const _: fn() -> FakeRunner = fake_runner_usage_marker;

/// Install the test stub for `jackin-capsule` so integration tests skip the download.
///
/// The library unit-test build uses `cfg!(test)` for the same purpose;
/// integration tests need to call this explicitly because `cfg(test)`
/// only affects the lib when compiled for the lib's own test target.
pub fn install_capsule_binary_stub(paths: &JackinPaths) {
    jackin_image::capsule_binary::install_test_stub(paths)
        .expect("install jackin-capsule test stub");
}

pub fn install_agent_binary_stubs(paths: &JackinPaths) {
    for agent in jackin_core::Agent::ALL {
        jackin_image::agent_binary::install_test_stub(paths, *agent)
            .expect("install agent binary stub");
    }
}

const _: fn(&JackinPaths) = install_capsule_binary_stub;
const _: fn(&JackinPaths) = install_agent_binary_stubs;

pub fn launched_role_container(docker: &FakeDockerClient) -> (String, ContainerSpec) {
    docker
        .created_containers
        .borrow()
        .iter()
        .find(|(_, spec)| {
            spec.labels
                .get("jackin.kind")
                .is_some_and(|value| value == "role")
        })
        .cloned()
        .expect("expected role container create request")
}
const _: fn(&FakeDockerClient) -> (String, ContainerSpec) = launched_role_container;
