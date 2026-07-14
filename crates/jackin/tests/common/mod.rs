#![expect(
    unreachable_pub,
    reason = "shared integration-test helper module: pub organizes fixtures, not a crate export"
)]

//! Shared test helpers for launch integration tests.
// pub items in a private test-helper module are intentionally unreachable
// from outside this crate; they exist for code organisation, not export.

use jackin_core::paths::JackinPaths;
use jackin_docker::docker_client::{
    ContainerRow, ContainerState, DockerApi, NetworkRow, RemoveImageOutcome,
};
use std::collections::HashMap;

/// Canonical `CommandRunner` fake (shared with host/runtime suites).
pub use jackin_test_support::FakeRunner;

// Keep the re-export live for integration crates that do not import FakeRunner
// themselves (`per_mount_isolation_e2e` etc.) — otherwise `-D unused-imports`
// fails those targets.
fn fake_runner_usage_marker() -> FakeRunner {
    FakeRunner::for_load_agent([String::new()])
}
const _: fn() -> FakeRunner = fake_runner_usage_marker;

/// Install the test stub for `jackin-capsule` so integration tests skip the download.
///
/// `cargo test` of the lib uses `cfg!(test)` for the same purpose;
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

/// Minimal no-op `DockerApi` stub. All operations return empty/success so
/// `load_role` proceeds as if no containers exist.
///
/// Kept local (not `FakeDockerClient`) — call sites only need empty success
/// defaults and do not exercise queue/`inspect_state_by_name` features.
#[derive(Debug)]
pub struct NoOpDocker;

impl DockerApi for NoOpDocker {
    async fn ping(&self) -> anyhow::Result<()> {
        Ok(())
    }

    async fn inspect_container_state(&self, _name: &str) -> ContainerState {
        ContainerState::NotFound
    }
    async fn remove_container(&self, _name: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn list_containers(
        &self,
        _label_filters: &[&str],
        _all: bool,
    ) -> anyhow::Result<Vec<ContainerRow>> {
        Ok(vec![])
    }
    async fn create_container(
        &self,
        _name: &str,
        _spec: jackin_docker::docker_client::ContainerSpec,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn start_container(&self, _name: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn remove_volume(&self, _name: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn create_network(
        &self,
        _name: &str,
        _labels: HashMap<String, String>,
        _internal: bool,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn remove_network(&self, _name: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn list_networks(&self, _label_filters: &[&str]) -> anyhow::Result<Vec<NetworkRow>> {
        Ok(vec![])
    }
    async fn inspect_network(&self, _name: &str) -> anyhow::Result<Option<NetworkRow>> {
        Ok(None)
    }
    async fn list_image_tags(&self, _reference_filter: &str) -> anyhow::Result<Vec<String>> {
        Ok(vec![])
    }
    async fn remove_image(&self, _name: &str) -> anyhow::Result<RemoveImageOutcome> {
        Ok(RemoveImageOutcome::NotFound)
    }
    async fn inspect_image_labels(&self, _image: &str) -> anyhow::Result<HashMap<String, String>> {
        Ok(HashMap::new())
    }
    async fn pull_image(&self, _image: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn exec_capture(&self, _container: &str, _cmd: &[&str]) -> anyhow::Result<String> {
        Ok(String::new())
    }
}
