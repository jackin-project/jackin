//! Shared test helpers for launch integration tests.
// pub items in a private test-helper module are intentionally unreachable
// from outside this crate; they exist for code organisation, not export.

#![expect(
    clippy::expect_used,
    reason = "integration test binary stub setup should fail immediately with source location"
)]
#![allow(unreachable_pub)]

use jackin::docker::{CommandRunner, RunOptions};
use jackin::docker_client::{
    ContainerRow, ContainerState, DockerApi, NetworkRow, RemoveImageOutcome,
};
use jackin::paths::JackinPaths;
use std::collections::{HashMap, VecDeque};
use std::path::Path;

/// Install the test stub for `jackin-capsule` so integration tests skip the download.
///
/// `cargo test` of the lib uses `cfg!(test)` for the same purpose;
/// integration tests need to call this explicitly because `cfg(test)`
/// only affects the lib when compiled for the lib's own test target.
pub fn install_capsule_binary_stub(paths: &JackinPaths) {
    jackin_image::capsule_binary::install_test_stub(paths).expect("install jackin-capsule test stub");
}

pub fn install_agent_binary_stubs(paths: &JackinPaths) {
    for agent in jackin::agent::Agent::ALL {
        jackin_image::agent_binary::install_test_stub(paths, *agent).expect("install agent binary stub");
    }
}

const _: fn(&JackinPaths) = install_capsule_binary_stub;
const _: fn(&JackinPaths) = install_agent_binary_stubs;

/// Minimal no-op `DockerApi` stub. All operations return empty/success so
/// `load_role` proceeds as if no containers exist.
#[derive(Debug)]
pub struct NoOpDocker;

impl DockerApi for NoOpDocker {
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
        _spec: jackin::docker_client::ContainerSpec,
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

/// Queue-based `CommandRunner` for `load_role` integration tests.
///
/// Pre-fills 4 empty slots for the identity-lookup preamble (git config
/// user.name/email, id -u/-g); GC calls now go through `DockerApi`, not `CommandRunner`.
#[derive(Debug, Default)]
pub struct FakeRunner {
    pub recorded: Vec<String>,
    pub capture_queue: VecDeque<String>,
}

impl FakeRunner {
    pub fn for_load_agent(outputs: impl IntoIterator<Item = String>) -> Self {
        let mut capture_queue = VecDeque::new();
        for _ in 0..4 {
            capture_queue.push_back(String::new());
        }
        capture_queue.extend(outputs);
        Self {
            recorded: Vec::new(),
            capture_queue,
        }
    }
}

fn fake_runner_usage_marker() -> FakeRunner {
    FakeRunner::for_load_agent(Vec::<String>::new())
}

const _: fn() -> FakeRunner = fake_runner_usage_marker;

impl CommandRunner for FakeRunner {
    async fn run(
        &mut self,
        program: &str,
        args: &[&str],
        _cwd: Option<&Path>,
        _opts: &RunOptions,
    ) -> anyhow::Result<()> {
        self.recorded.push(format!("{program} {}", args.join(" ")));
        Ok(())
    }

    async fn capture(
        &mut self,
        program: &str,
        args: &[&str],
        _cwd: Option<&Path>,
    ) -> anyhow::Result<String> {
        self.recorded.push(format!("{program} {}", args.join(" ")));
        Ok(self.capture_queue.pop_front().unwrap_or_default())
    }

    async fn capture_secret(
        &mut self,
        program: &str,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> anyhow::Result<String> {
        self.capture(program, args, cwd).await
    }
}
