// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn authority(paths: &JackinPaths) -> PathBuf {
    let directory = coordination::universe_dir(paths).unwrap();
    // Fixtures create the same private namespace as production without
    // selecting a second root or mutating its boundary generation.
    drop(coordination::open_in_namespace(&directory, "universe-lock").unwrap());
    directory
}

pub(super) fn seed_marker(paths: &JackinPaths, kind: StartKind) {
    let _lock = boundary_lock(&authority(paths)).unwrap();
    advance_generation(&authority(paths)).unwrap();
    mark_start_locked(&authority(paths), kind).unwrap();
}

thread_local! {
    pub(super) static DOCKER_GENERATION_CHURN: std::cell::RefCell<Option<PathBuf>> = const {
        std::cell::RefCell::new(None)
    };
}

pub(super) fn advance_generation_during_docker_list(operation: &str) {
    if operation.starts_with("docker ps") {
        DOCKER_GENERATION_CHURN.with(|slot| {
            if let Some(authority_dir) = slot.borrow().as_ref() {
                let _lock = boundary_lock(authority_dir).unwrap();
                advance_generation(authority_dir).unwrap();
            }
        });
    }
}

#[cfg(unix)]
pub(super) fn wait_for_fixture_path(path: &Path, message: &str) {
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    while !path.exists() {
        assert!(std::time::Instant::now() < deadline, "{message}");
        #[expect(
            clippy::disallowed_methods,
            reason = "bounded process-fixture polling runs on an ordinary synchronous test thread"
        )]
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(unix)]
pub(super) struct PendingOwnerProcess(pub(super) std::process::Child);

#[cfg(unix)]
impl Drop for PendingOwnerProcess {
    fn drop(&mut self) {
        let _kill_result = self.0.kill();
        let _wait_result = self.0.wait();
    }
}

#[cfg(unix)]
pub(super) fn spawn_pending_owner(root: &Path) -> PendingOwnerProcess {
    let executable = std::env::current_exe().unwrap();
    let child = std::process::Command::new(executable)
        .args([
            "--exact",
            "universe::tests::case_02::pending_owner_process_worker",
            "--nocapture",
        ])
        .env("JACKIN_TEST_PENDING_OWNER_ROOT", root)
        .spawn()
        .unwrap();
    PendingOwnerProcess(child)
}

#[cfg(unix)]
pub(super) fn only_pending_claim(authority: &Path) -> PathBuf {
    let mut entries = std::fs::read_dir(pending_dir(authority)).unwrap();
    let path = entries.next().unwrap().unwrap().path();
    assert!(entries.next().is_none(), "expected one pending claim");
    path
}

#[cfg(unix)]
pub(super) struct CausalInterleavingDocker {
    pub(super) fake: FakeDockerClient,
    pub(super) current_containers: std::sync::Arc<std::sync::Mutex<Vec<ContainerRow>>>,
    pub(super) observations: std::sync::Arc<std::sync::Mutex<Vec<Vec<ContainerRow>>>>,
    pub(super) first_observation: std::sync::mpsc::Sender<()>,
    pub(super) release_first_observation: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
}

#[cfg(unix)]
impl DockerApi for CausalInterleavingDocker {
    #[expect(
        clippy::manual_async_fn,
        reason = "desugared so the sync-bodied fake satisfies 1.98's \
         `unused_async_trait_impl`; the trait still requires a `Future`"
    )]
    fn list_containers(
        &self,
        _label_filters: &[&str],
        _all: bool,
    ) -> impl Future<Output = anyhow::Result<Vec<ContainerRow>>> {
        async move {
            let snapshot = self.current_containers.lock().unwrap().clone();
            let call_number = {
                let mut observations = self.observations.lock().unwrap();
                observations.push(snapshot.clone());
                observations.len()
            };
            if call_number == 1 {
                self.first_observation.send(()).unwrap();
                self.release_first_observation
                    .lock()
                    .unwrap()
                    .recv()
                    .unwrap();
            }
            Ok(snapshot)
        }
    }

    forward_docker_api_methods! {
        ping() -> anyhow::Result<()>,
        inspect_container_by_name(name: &str) -> ContainerInspection,
        inspect_container_by_id(container: &ContainerHandle) -> ContainerState,
        remove_container_by_id(container: &ContainerHandle) -> anyhow::Result<()>,
        create_container(name: &str, spec: ContainerSpec) -> anyhow::Result<ContainerHandle>,
        start_container_by_id(container: &ContainerHandle) -> anyhow::Result<()>,
        remove_volume(name: &str) -> anyhow::Result<()>,
        create_network(name: &str, labels: HashMap<String, String>, internal: bool) -> anyhow::Result<()>,
        remove_network(name: &str) -> anyhow::Result<()>,
        list_networks(label_filters: &[&str]) -> anyhow::Result<Vec<NetworkRow>>,
        inspect_network(name: &str) -> anyhow::Result<Option<NetworkRow>>,
        list_image_tags(reference_filter: &str) -> anyhow::Result<Vec<String>>,
        remove_image(name: &str) -> anyhow::Result<RemoveImageOutcome>,
        inspect_image_labels(image: &str) -> anyhow::Result<HashMap<String, String>>,
        pull_image(image: &str) -> anyhow::Result<()>,
        exec_capture_by_id(container: &ContainerHandle, cmd: &[&str]) -> anyhow::Result<String>,
    }
}
