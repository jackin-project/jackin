// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `universe`.
use super::*;

#[test]
fn exit_claim_recovery_export_is_bodyless() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    tracing::subscriber::with_default(subscriber, record_exit_claim_recovery);

    export.force_flush();
    assert_eq!(export.event_count("operation.warn"), 1);
    assert!(export.contains_log_text("recovered_degradation"));
    for private in ["marker", "claim", "permission", "path", "raw error"] {
        assert!(!export.contains_log_text(private));
    }
}
use jackin_docker::docker_client::{
    ContainerHandle, ContainerInspection, ContainerRow, ContainerSpec, ContainerState, DockerApi,
    NetworkRow, RemoveImageOutcome,
};
use jackin_test_support::FakeDockerClient;
use std::collections::{HashMap, VecDeque};

fn authority(paths: &JackinPaths) -> PathBuf {
    let directory = super::super::coordination::universe_dir(paths).unwrap();
    // Fixtures create the same private namespace as production without
    // selecting a second root or mutating its boundary generation.
    drop(super::super::coordination::open_in_namespace(&directory, "universe-lock").unwrap());
    directory
}

fn seed_marker(paths: &JackinPaths, kind: StartKind) {
    let _lock = boundary_lock(&authority(paths)).unwrap();
    advance_generation(&authority(paths)).unwrap();
    mark_start_locked(&authority(paths), kind).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn universe_auxiliary_symlinks_cannot_redirect_state_operations() {
    for key in ["universe-generation", "universe-since", "universe-pending"] {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let directory = authority(&paths);
        let outside = tmp.path().join("outside");
        std::fs::create_dir(&outside).unwrap();
        let sentinel = outside.join("sentinel");
        std::fs::write(&sentinel, "untouched").unwrap();
        let target = if key == "universe-pending" {
            &outside
        } else {
            &sentinel
        };
        std::os::unix::fs::symlink(target, directory.join(key)).unwrap();
        let docker = FakeDockerClient::default();

        let claim = claim_entry(&paths, &docker).await;
        mark_start(&paths, StartKind::FreshConstruct).await;
        let (_, exit) = observe_exit(&paths, &docker).await.unwrap();

        assert_eq!(
            claim.start_kind(),
            StartKind::ResumeExisting,
            "unsafe {key}"
        );
        assert!(
            claim.pending_file.is_none(),
            "unsafe {key} must not own a redirected token"
        );
        assert_eq!(exit, ExitClaim::Missing);
        assert_eq!(std::fs::read_to_string(&sentinel).unwrap(), "untouched");
        assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 1);
        assert!(
            std::fs::symlink_metadata(directory.join(key))
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn universe_auxiliary_nonregular_inodes_fail_closed() {
    for key in ["universe-generation", "universe-since"] {
        for fifo in [false, true] {
            let tmp = tempfile::tempdir().unwrap();
            let paths = JackinPaths::for_tests(tmp.path());
            paths.ensure_base_dirs().unwrap();
            let directory = authority(&paths);
            let invalid = directory.join(key);
            if fifo {
                nix::unistd::mkfifo(&invalid, nix::sys::stat::Mode::from_bits_truncate(0o600))
                    .unwrap();
            } else {
                std::fs::create_dir(&invalid).unwrap();
            }
            let docker = FakeDockerClient::default();

            let claim = claim_entry(&paths, &docker).await;
            let (_, exit) = observe_exit(&paths, &docker).await.unwrap();

            assert_eq!(claim.start_kind(), StartKind::ResumeExisting);
            assert!(claim.pending_file.is_none());
            assert_eq!(exit, ExitClaim::Missing);
            std::fs::symlink_metadata(&invalid).unwrap();
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn universe_auxiliary_hardlinks_cannot_modify_external_state() {
    use std::os::unix::fs::PermissionsExt as _;

    for key in ["universe-generation", "universe-since"] {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let directory = authority(&paths);
        let sentinel = tmp.path().join("sentinel");
        std::fs::write(&sentinel, "untouched").unwrap();
        std::fs::set_permissions(&sentinel, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::hard_link(&sentinel, directory.join(key)).unwrap();

        let claim = claim_entry(&paths, &FakeDockerClient::default()).await;
        mark_start(&paths, StartKind::FreshConstruct).await;

        assert_eq!(claim.start_kind(), StartKind::ResumeExisting);
        assert!(claim.pending_file.is_none());
        assert_eq!(std::fs::read_to_string(&sentinel).unwrap(), "untouched");
    }
}

#[cfg(unix)]
#[tokio::test]
async fn universe_auxiliary_state_is_private_and_owned() {
    use std::os::unix::fs::MetadataExt as _;

    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let claim = claim_entry(&paths, &FakeDockerClient::default()).await;
    let directory = authority(&paths);
    for file in [
        directory.join("universe-generation"),
        marker_path(&directory),
        claim.pending_file.clone().unwrap(),
    ] {
        let metadata = std::fs::metadata(file).unwrap();
        assert_eq!(metadata.mode() & 0o777, 0o600);
        assert_eq!(metadata.uid(), nix::unistd::geteuid().as_raw());
        assert_eq!(metadata.nlink(), 1);
    }
    for path in [&directory, &pending_dir(&directory)] {
        let metadata = std::fs::metadata(path).unwrap();
        assert_eq!(metadata.mode() & 0o777, 0o700);
        assert_eq!(metadata.uid(), nix::unistd::geteuid().as_raw());
    }
}

#[cfg(unix)]
#[tokio::test]
async fn redirected_owned_pending_token_blocks_lifecycle_mutation() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let docker = FakeDockerClient::default();
    let claim = claim_entry(&paths, &docker).await;
    let directory = authority(&paths);
    let pending = claim.pending_file.clone().unwrap();
    let before = generation(&directory).unwrap();
    let sentinel = tmp.path().join("sentinel");
    std::fs::write(&sentinel, "untouched").unwrap();
    std::fs::remove_file(&pending).unwrap();
    std::os::unix::fs::symlink(&sentinel, &pending).unwrap();

    assert!(claim.activate().await.is_err());
    release_entry_if_idle(&docker, &claim).await;
    drop(claim);

    assert_eq!(generation(&directory).unwrap(), before);
    assert_eq!(std::fs::read_to_string(&sentinel).unwrap(), "untouched");
    assert!(
        std::fs::symlink_metadata(&pending)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(marker_path(&directory).exists());
}

#[test]
fn boundary_guard_excludes_independent_file_descriptors() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let guard = boundary_lock(&authority(&paths)).unwrap();
    #[expect(
        clippy::disallowed_methods,
        reason = "synchronous OS-lock fixture runs on an ordinary test thread, outside async or render work"
    )]
    let contender = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(authority(&paths).join("universe-lock.lock"))
        .unwrap();

    assert!(
        contender.try_lock().is_err(),
        "boundary guard must acquire the persistent file lock"
    );
    drop(guard);
    contender.try_lock().unwrap();
}

#[test]
fn universe_lock_authority_survives_full_runtime_home_prune() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let directory = authority(&paths);
    let guard = boundary_lock(&directory).unwrap();
    super::super::coordination::ensure_prunable(&paths, &paths.jackin_home).unwrap();

    std::fs::remove_dir_all(&paths.jackin_home).unwrap();
    paths.ensure_base_dirs().unwrap();

    assert_eq!(authority(&paths), directory);
    let contender =
        super::super::coordination::open_in_namespace(&directory, "universe-lock").unwrap();
    assert!(
        contender.try_lock().is_err(),
        "prune must preserve the held lock inode"
    );
    drop(guard);
    contender.try_lock().unwrap();
}

#[tokio::test]
async fn universe_pending_generation_and_marker_survive_data_prune() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let docker = FakeDockerClient::default();
    let first = claim_entry(&paths, &docker).await;
    let directory = authority(&paths);
    let pending = first.pending_file.clone().unwrap();
    let previous_generation = generation(&directory).unwrap();
    let marker = std::fs::read_to_string(marker_path(&directory)).unwrap();
    super::super::coordination::ensure_prunable(&paths, &paths.data_dir).unwrap();

    std::fs::remove_dir_all(&paths.data_dir).unwrap();
    paths.ensure_base_dirs().unwrap();

    assert_eq!(authority(&paths), directory);
    assert!(pending.exists());
    assert_eq!(generation(&directory).unwrap(), previous_generation);
    assert_eq!(
        std::fs::read_to_string(marker_path(&directory)).unwrap(),
        marker
    );
    let second = claim_entry(&paths, &docker).await;
    assert_eq!(second.start_kind(), StartKind::ResumeExisting);
    release_entry_if_idle(&docker, &first).await;
    assert!(second.pending_file.as_ref().unwrap().exists());
    assert_eq!(
        std::fs::read_to_string(marker_path(&directory)).unwrap(),
        marker
    );
    second.activate().await.unwrap();
    let (_, exit) = observe_exit(&paths, &docker).await.unwrap();
    assert!(matches!(exit, ExitClaim::Claimed { .. }));
}

thread_local! {
    static DOCKER_GENERATION_CHURN: std::cell::RefCell<Option<PathBuf>> = const {
        std::cell::RefCell::new(None)
    };
}

fn advance_generation_during_docker_list(operation: &str) {
    if operation.starts_with("docker ps") {
        DOCKER_GENERATION_CHURN.with(|slot| {
            if let Some(authority_dir) = slot.borrow().as_ref() {
                let _lock = boundary_lock(authority_dir).unwrap();
                advance_generation(authority_dir).unwrap();
            }
        });
    }
}

#[tokio::test]
async fn entry_observation_churn_keeps_an_owned_pending_lease() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let docker = FakeDockerClient {
        operation_hook: Some(advance_generation_during_docker_list),
        ..Default::default()
    };
    DOCKER_GENERATION_CHURN.with(|slot| *slot.borrow_mut() = Some(authority(&paths)));

    let claim = claim_entry(&paths, &docker).await;

    DOCKER_GENERATION_CHURN.with(|slot| *slot.borrow_mut() = None);
    assert_eq!(docker.recorded.borrow().len(), ENTRY_OBSERVATION_ATTEMPTS);
    assert_eq!(claim.start_kind(), StartKind::ResumeExisting);
    assert!(claim.pending_file.as_ref().unwrap().exists());
    assert_eq!(count_pending_claims(&authority(&paths)), Some(1));
    drop(claim);
    assert_eq!(count_pending_claims(&authority(&paths)), Some(0));
}

#[tokio::test]
async fn exit_observation_rejects_generation_changed_during_docker_request() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    seed_marker(&paths, StartKind::FreshConstruct);
    let docker = FakeDockerClient {
        operation_hook: Some(advance_generation_during_docker_list),
        ..Default::default()
    };
    DOCKER_GENERATION_CHURN.with(|slot| *slot.borrow_mut() = Some(authority(&paths)));

    let (running, claim) = observe_exit(&paths, &docker).await.unwrap();

    DOCKER_GENERATION_CHURN.with(|slot| *slot.borrow_mut() = None);
    assert!(running.is_empty());
    assert_eq!(claim, ExitClaim::Missing);
    assert!(marker_path(&authority(&paths)).exists());
    let (_, claim) = observe_exit(&paths, &docker).await.unwrap();
    assert!(matches!(claim, ExitClaim::Claimed { .. }));
}

#[tokio::test]
async fn activated_entry_allows_exit_before_the_owned_launch_lease_drops() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let docker = FakeDockerClient::default();
    let claim = claim_entry(&paths, &docker).await;
    let pending_file = claim.pending_file.clone().unwrap();

    claim.activate().await.unwrap();

    assert!(!pending_file.exists());
    assert!(marker_path(&authority(&paths)).exists());
    let (_, exit) = observe_exit(&paths, &docker).await.unwrap();
    assert!(matches!(exit, ExitClaim::Claimed { .. }));
    // The app/options object is still alive through foreground exit rendering.
    assert_eq!(claim.start_kind(), StartKind::FreshConstruct);
    drop(claim);
    assert!(!marker_path(&authority(&paths)).exists());
}

#[tokio::test]
async fn cancelling_launch_future_releases_its_owned_pending_entry() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let docker = FakeDockerClient::default();
    let (claimed_tx, mut claimed_rx) = tokio::sync::oneshot::channel();
    let mut launch = Box::pin(async {
        let claim = claim_entry(&paths, &docker).await;
        claimed_tx.send(()).unwrap();
        std::future::pending::<()>().await;
        drop(claim);
    });
    tokio::select! {
        biased;
        () = &mut launch => panic!("launch should await its foreground session"),
        result = &mut claimed_rx => result.unwrap()
    }
    assert_eq!(count_pending_claims(&authority(&paths)), Some(1));

    drop(launch);

    assert_eq!(count_pending_claims(&authority(&paths)), Some(0));
    assert!(marker_path(&authority(&paths)).exists());
}

#[tokio::test]
async fn cancelled_worker_result_drops_its_owned_pending_entry() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let (created_tx, mut created_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let directory = authority(&paths);
    let mut transaction = Box::pin(boundary_work(&directory, move |authority_dir| {
        let claim = {
            let _lock = boundary_lock(authority_dir)?;
            advance_generation(authority_dir)?;
            register_pending_entry_locked(authority_dir, true)?.ok_or_else(|| {
                std::io::Error::other("fresh pending registration requested a retry")
            })?
        };
        created_tx.send(()).unwrap();
        // Hold the completed owned value outside the file lock until the
        // caller cancels its wait for this worker's result.
        release_rx.recv().unwrap();
        Ok(claim)
    }));
    tokio::select! {
        biased;
        result = &mut transaction => panic!("worker unexpectedly returned: {result:?}"),
        result = &mut created_rx => result.unwrap()
    }
    assert_eq!(count_pending_claims(&authority(&paths)), Some(1));

    drop(transaction);
    release_tx.send(()).unwrap();

    tokio::time::timeout(Duration::from_secs(5), async {
        while has_pending_claims(&authority(&paths)) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(marker_path(&authority(&paths)).exists());
}

#[test]
fn concurrent_entry_claims_have_exactly_one_fresh_winner() {
    use std::sync::{Arc, Barrier};

    let tmp = tempfile::tempdir().unwrap();
    let paths = Arc::new(JackinPaths::for_tests(tmp.path()));
    paths.ensure_base_dirs().unwrap();
    let barrier = Arc::new(Barrier::new(8));
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let paths = Arc::clone(&paths);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .build()
                    .unwrap();
                let docker = FakeDockerClient::default();
                barrier.wait();
                runtime.block_on(claim_entry(&paths, &docker))
            })
        })
        .collect();
    let claims: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect();
    assert_eq!(
        claims
            .iter()
            .filter(|claim| claim.start_kind() == StartKind::FreshConstruct)
            .count(),
        1
    );
    assert_eq!(count_pending_claims(&authority(&paths)), Some(8));
    drop(claims);
    assert_eq!(count_pending_claims(&authority(&paths)), Some(0));
}

#[tokio::test]
async fn stale_idle_observation_cannot_remove_a_newer_live_marker() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let docker = FakeDockerClient::default();
    let first = claim_entry(&paths, &docker).await;
    let observed_generation = {
        let _lock = boundary_lock(&authority(&paths)).unwrap();
        let generation = advance_generation(&authority(&paths)).unwrap();
        std::fs::remove_file(first.pending_file.as_ref().unwrap()).unwrap();
        generation
    };
    // A new launch completes while the first release's Docker request is in
    // flight. No pending token remains, so generation is the required guard.
    let second = claim_entry(&paths, &docker).await;
    drop(second);
    assert_eq!(count_pending_claims(&authority(&paths)), Some(0));
    let marker = std::fs::read_to_string(marker_path(&authority(&paths))).unwrap();

    release_marker_if_unchanged(&authority(&paths), &observed_generation);

    assert_eq!(
        std::fs::read_to_string(marker_path(&authority(&paths))).unwrap(),
        marker
    );
    drop(first);
}

#[tokio::test]
async fn exit_claim_does_not_consume_a_pending_launch_boundary() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let claim = claim_entry(&paths, &FakeDockerClient::default()).await;

    assert_eq!(take_exit_claim(&paths), ExitClaim::Missing);
    assert!(claim.pending_file.as_ref().unwrap().exists());
    assert!(marker_path(&authority(&paths)).exists());
    drop(claim);
    assert!(matches!(take_exit_claim(&paths), ExitClaim::Claimed { .. }));
}

#[cfg(unix)]
fn wait_for_fixture_path(path: &Path, message: &str) {
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
struct PendingOwnerProcess(std::process::Child);

#[cfg(unix)]
impl Drop for PendingOwnerProcess {
    fn drop(&mut self) {
        let _kill_result = self.0.kill();
        let _wait_result = self.0.wait();
    }
}

#[cfg(unix)]
fn spawn_pending_owner(root: &Path) -> PendingOwnerProcess {
    let executable = std::env::current_exe().unwrap();
    let child = std::process::Command::new(executable)
        .args([
            "--exact",
            "runtime::universe::tests::pending_owner_process_worker",
            "--nocapture",
        ])
        .env("JACKIN_TEST_PENDING_OWNER_ROOT", root)
        .spawn()
        .unwrap();
    PendingOwnerProcess(child)
}

#[cfg(unix)]
fn only_pending_claim(authority: &Path) -> PathBuf {
    let mut entries = std::fs::read_dir(pending_dir(authority)).unwrap();
    let path = entries.next().unwrap().unwrap().path();
    assert!(entries.next().is_none(), "expected one pending claim");
    path
}

#[cfg(unix)]
macro_rules! forward_docker_api_methods {
    ($($method:ident($($argument:ident: $argument_type:ty),*) -> $output:ty),* $(,)?) => {
        $(
            async fn $method(&self $(, $argument: $argument_type)*) -> $output {
                self.fake.$method($($argument),*).await
            }
        )*
    };
}

#[cfg(unix)]
struct CausalInterleavingDocker {
    fake: FakeDockerClient,
    current_containers: std::sync::Arc<std::sync::Mutex<Vec<ContainerRow>>>,
    observations: std::sync::Arc<std::sync::Mutex<Vec<Vec<ContainerRow>>>>,
    first_observation: std::sync::mpsc::Sender<()>,
    release_first_observation: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
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

#[cfg(unix)]
#[test]
fn pending_owner_process_worker() {
    let Some(root) = std::env::var_os("JACKIN_TEST_PENDING_OWNER_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let paths = JackinPaths::for_tests(&root);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let claim = runtime.block_on(claim_entry(&paths, &FakeDockerClient::default()));
    assert!(claim.pending_file.is_some());
    std::fs::write(root.join("pending-owner-ready"), "").unwrap();
    wait_for_fixture_path(
        &root.join("pending-owner-release"),
        "parent did not release the pending owner",
    );
    drop(claim);
}

#[cfg(unix)]
#[test]
fn live_pending_owner_is_not_pruned_or_claimed_for_exit() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut owner = spawn_pending_owner(tmp.path());
    wait_for_fixture_path(
        &tmp.path().join("pending-owner-ready"),
        "pending owner did not create its claim",
    );

    let directory = authority(&paths);
    let pending = only_pending_claim(&directory);
    let key = pending.file_name().unwrap().to_str().unwrap();
    let probe =
        super::super::coordination::open_state_in_namespace(&pending_dir(&directory), key, false)
            .unwrap();
    assert!(
        matches!(probe.try_lock(), Err(std::fs::TryLockError::WouldBlock)),
        "live owner must hold its pending lease"
    );
    drop(probe);

    let observed_generation = generation(&directory).unwrap();
    {
        let _lock = boundary_lock(&directory).unwrap();
        assert!(!prune_stale_pending_claims(&directory).unwrap());
    }
    assert!(pending.exists(), "live pending token must be preserved");
    assert_eq!(take_exit_claim(&paths), ExitClaim::Missing);
    assert_eq!(generation(&directory).unwrap(), observed_generation);

    std::fs::write(tmp.path().join("pending-owner-release"), "").unwrap();
    assert!(owner.0.wait().unwrap().success());
    assert!(!pending.exists(), "owner drop must remove its lease");
    assert!(matches!(take_exit_claim(&paths), ExitClaim::Claimed { .. }));
}

#[cfg(unix)]
#[tokio::test]
async fn killed_pending_owner_is_recovered_before_exit_claim() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut owner = spawn_pending_owner(tmp.path());
    wait_for_fixture_path(
        &tmp.path().join("pending-owner-ready"),
        "pending owner did not create its claim",
    );

    let directory = authority(&paths);
    let pending = only_pending_claim(&directory);
    let key = pending.file_name().unwrap().to_str().unwrap();
    let probe =
        super::super::coordination::open_state_in_namespace(&pending_dir(&directory), key, false)
            .unwrap();
    assert!(
        matches!(probe.try_lock(), Err(std::fs::TryLockError::WouldBlock)),
        "claim should be owned before killing its process"
    );
    drop(probe);
    let observed_generation = generation(&directory).unwrap();

    owner.0.kill().unwrap();
    assert!(!owner.0.wait().unwrap().success());
    assert!(pending.exists(), "SIGKILL must leave the pending inode");
    assert_eq!(generation(&directory).unwrap(), observed_generation);

    assert_eq!(take_exit_claim(&paths), ExitClaim::Missing);
    assert!(!pending.exists(), "next exit must reclaim the orphan token");
    assert_eq!(count_pending_claims(&directory), Some(0));
    assert!(marker_path(&directory).exists());
    assert!(matches!(take_exit_claim(&paths), ExitClaim::Claimed { .. }));

    let next_claim = claim_entry(&paths, &FakeDockerClient::default()).await;
    assert_eq!(next_claim.start_kind(), StartKind::FreshConstruct);
    drop(next_claim);
    assert!(matches!(take_exit_claim(&paths), ExitClaim::Claimed { .. }));
}

#[cfg(unix)]
#[tokio::test]
async fn observe_exit_rechecks_docker_after_reclaiming_dead_owner() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut owner = spawn_pending_owner(tmp.path());
    wait_for_fixture_path(
        &tmp.path().join("pending-owner-ready"),
        "pending owner did not create its claim",
    );

    let directory = authority(&paths);
    let pending = only_pending_claim(&directory);
    let previous_generation = generation(&directory).unwrap();
    owner.0.kill().unwrap();
    assert!(!owner.0.wait().unwrap().success());
    assert!(pending.exists(), "SIGKILL leaves the pending inode");

    // Script an empty stale snapshot followed by a live container. The same
    // observe_exit call must list Docker again after reclaiming the token.
    let docker = FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([
            vec![],
            vec![ContainerRow {
                name: "jk-running".to_owned(),
                id: "container-id".to_owned(),
                labels: HashMap::new(),
            }],
        ])),
        ..Default::default()
    };

    let (running, claim) = observe_exit(&paths, &docker).await.unwrap();

    assert_eq!(running, vec!["jk-running".to_owned()]);
    assert_eq!(claim, ExitClaim::Missing);
    assert_eq!(docker.recorded.borrow().len(), 2);
    assert!(!pending.exists(), "stale token was reclaimed");
    assert_eq!(count_pending_claims(&directory), Some(0));
    assert_ne!(generation(&directory).unwrap(), previous_generation);
    assert!(marker_path(&directory).exists(), "live marker is preserved");
}

#[cfg(unix)]
#[tokio::test]
async fn observe_exit_claims_after_recovery_when_fresh_docker_view_is_empty() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut owner = spawn_pending_owner(tmp.path());
    wait_for_fixture_path(
        &tmp.path().join("pending-owner-ready"),
        "pending owner did not create its claim",
    );

    let directory = authority(&paths);
    let pending = only_pending_claim(&directory);
    owner.0.kill().unwrap();
    assert!(!owner.0.wait().unwrap().success());

    let docker = FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![], vec![]])),
        ..Default::default()
    };

    let (running, claim) = observe_exit(&paths, &docker).await.unwrap();

    assert!(running.is_empty());
    assert!(matches!(claim, ExitClaim::Claimed { .. }));
    assert_eq!(docker.recorded.borrow().len(), 2);
    assert!(!pending.exists());
    assert_eq!(count_pending_claims(&directory), Some(0));
    assert!(!marker_path(&directory).exists());
}

#[cfg(unix)]
#[tokio::test]
async fn entry_rechecks_docker_after_reclaiming_dead_owner() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut owner = spawn_pending_owner(tmp.path());
    wait_for_fixture_path(
        &tmp.path().join("pending-owner-ready"),
        "pending owner did not create its claim",
    );

    let directory = authority(&paths);
    let stale_pending = only_pending_claim(&directory);
    owner.0.kill().unwrap();
    assert!(!owner.0.wait().unwrap().success());

    let docker = FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([
            vec![],
            vec![ContainerRow {
                name: "jk-running".to_owned(),
                id: "container-id".to_owned(),
                labels: HashMap::new(),
            }],
        ])),
        ..Default::default()
    };

    let claim = claim_entry(&paths, &docker).await;

    assert_eq!(claim.start_kind(), StartKind::ResumeExisting);
    assert_eq!(docker.recorded.borrow().len(), 2);
    assert!(!stale_pending.exists(), "stale token was reclaimed");
    assert_eq!(count_pending_claims(&directory), Some(1));
    assert!(marker_path(&directory).exists());
    drop(claim);
}

#[cfg(unix)]
#[test]
fn entry_reobserves_container_started_after_stale_empty_snapshot_before_owner_kill() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut owner = spawn_pending_owner(tmp.path());
    wait_for_fixture_path(
        &tmp.path().join("pending-owner-ready"),
        "pending owner did not create its claim",
    );

    let directory = authority(&paths);
    let stale_pending = only_pending_claim(&directory);
    let marker_before = std::fs::read_to_string(marker_path(&directory)).unwrap();
    let (first_observation_tx, first_observation_rx) = std::sync::mpsc::channel();
    let (release_first_observation_tx, release_first_observation_rx) = std::sync::mpsc::channel();
    let current_containers = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let observations = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let docker = CausalInterleavingDocker {
        fake: FakeDockerClient::default(),
        current_containers: std::sync::Arc::clone(&current_containers),
        observations: std::sync::Arc::clone(&observations),
        first_observation: first_observation_tx,
        release_first_observation: std::sync::Mutex::new(release_first_observation_rx),
    };

    let entrant = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        runtime.block_on(claim_entry(&paths, &docker))
    });

    first_observation_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("entrant did not capture its first Docker snapshot");
    let pending_key = stale_pending.file_name().unwrap().to_str().unwrap();
    let live_lease = super::super::coordination::open_state_in_namespace(
        &pending_dir(&directory),
        pending_key,
        false,
    )
    .unwrap();
    assert!(
        matches!(
            live_lease.try_lock(),
            Err(std::fs::TryLockError::WouldBlock)
        ),
        "the stale empty snapshot must be captured while the pending owner is live"
    );
    drop(live_lease);

    *current_containers.lock().unwrap() = vec![ContainerRow {
        name: "jk-running".to_owned(),
        id: "container-id".to_owned(),
        labels: HashMap::new(),
    }];
    owner.0.kill().unwrap();
    assert!(!owner.0.wait().unwrap().success());
    assert!(stale_pending.exists(), "SIGKILL leaves the pending inode");
    release_first_observation_tx.send(()).unwrap();

    let entrant = entrant.join().unwrap();
    assert_eq!(entrant.start_kind(), StartKind::ResumeExisting);
    assert!(
        entrant.pending_file.as_ref().unwrap().exists(),
        "the reobserving entrant owns its pending lease"
    );
    assert!(!stale_pending.exists(), "the orphan token was reclaimed");
    assert_eq!(count_pending_claims(&directory), Some(1));
    let observations = observations.lock().unwrap();
    assert_eq!(
        observations.len(),
        2,
        "Docker must be queried again after reclaim"
    );
    assert!(
        observations[0].is_empty(),
        "first observation is stale and empty"
    );
    assert_eq!(
        observations[1]
            .iter()
            .map(|container| container.name.as_str())
            .collect::<Vec<_>>(),
        vec!["jk-running"],
        "fresh observation sees the container started after the first snapshot"
    );
    drop(observations);
    assert_eq!(
        std::fs::read_to_string(marker_path(&directory)).unwrap(),
        marker_before,
        "recovery must preserve the marker for the running construct"
    );

    drop(entrant);
    assert_eq!(count_pending_claims(&directory), Some(0));
    assert_eq!(
        std::fs::read_to_string(marker_path(&directory)).unwrap(),
        marker_before
    );
}

#[test]
fn entry_claim_process_worker() {
    let Some(root) = std::env::var_os("JACKIN_TEST_ENTRY_PROCESS_ROOT") else {
        return;
    };
    let index = std::env::var("JACKIN_TEST_ENTRY_PROCESS_INDEX").unwrap();
    let root = PathBuf::from(root);
    let paths = JackinPaths::for_tests(&root);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    std::fs::write(root.join(format!("ready-{index}")), "").unwrap();
    while !root.join("start").exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "parent did not release start gate"
        );
        #[expect(
            clippy::disallowed_methods,
            reason = "bounded process fixture polling runs on ordinary synchronous test threads"
        )]
        std::thread::sleep(Duration::from_millis(5));
    }
    let claim = runtime.block_on(claim_entry(&paths, &FakeDockerClient::default()));
    std::fs::write(
        root.join(format!("result-{index}")),
        format!("{:?}", claim.start_kind()),
    )
    .unwrap();
    while !root.join("release").exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "parent did not release claims"
        );
        #[expect(
            clippy::disallowed_methods,
            reason = "bounded process fixture polling runs on ordinary synchronous test threads"
        )]
        std::thread::sleep(Duration::from_millis(5));
    }
    drop(claim);
}

#[test]
fn independent_processes_elect_one_fresh_entry() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let executable = std::env::current_exe().unwrap();
    let mut children: Vec<_> = (0..4)
        .map(|index| {
            std::process::Command::new(&executable)
                .args([
                    "--exact",
                    "runtime::universe::tests::entry_claim_process_worker",
                    "--nocapture",
                ])
                .env("JACKIN_TEST_ENTRY_PROCESS_ROOT", tmp.path())
                .env("JACKIN_TEST_ENTRY_PROCESS_INDEX", index.to_string())
                .spawn()
                .unwrap()
        })
        .collect();
    let wait_for_files = |prefix: &str| {
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        while !(0..4).all(|index| tmp.path().join(format!("{prefix}-{index}")).exists()) {
            assert!(
                std::time::Instant::now() < deadline,
                "children did not write {prefix}"
            );
            #[expect(
                clippy::disallowed_methods,
                reason = "bounded process fixture polling runs on ordinary synchronous test threads"
            )]
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    wait_for_files("ready");
    std::fs::write(tmp.path().join("start"), "").unwrap();
    wait_for_files("result");
    let results: Vec<_> = (0..4)
        .map(|index| std::fs::read_to_string(tmp.path().join(format!("result-{index}"))).unwrap())
        .collect();
    let pending = count_pending_claims(&authority(&paths));
    std::fs::write(tmp.path().join("release"), "").unwrap();
    for child in &mut children {
        assert!(child.wait().unwrap().success());
    }
    assert_eq!(
        results
            .iter()
            .filter(|kind| kind.as_str() == "FreshConstruct")
            .count(),
        1
    );
    assert_eq!(pending, Some(4));
    assert_eq!(count_pending_claims(&authority(&paths)), Some(0));
}

#[test]
fn mark_then_take_round_trips_and_clears() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();

    seed_marker(&paths, StartKind::FreshConstruct);
    assert!(marker_path(&authority(&paths)).exists(), "marker written");

    let ExitClaim::Claimed {
        elapsed: Some(elapsed),
    } = take_exit_claim(&paths)
    else {
        panic!("elapsed claim available");
    };
    assert!(
        elapsed < Duration::from_secs(5),
        "just-started span is small"
    );
    assert!(
        !marker_path(&authority(&paths)).exists(),
        "marker cleared after take"
    );
    assert_eq!(
        take_exit_claim(&paths),
        ExitClaim::Missing,
        "second take is empty"
    );
}

#[test]
fn env_flag_falsey_values_are_disabled() {
    for value in [
        None,
        Some(""),
        Some("0"),
        Some("false"),
        Some("no"),
        Some("off"),
    ] {
        assert!(
            !env_flag_enabled(value),
            "value should be falsey: {value:?}"
        );
    }
}

#[test]
fn env_flag_truthy_values_are_enabled() {
    for value in [Some("1"), Some("true"), Some("yes"), Some("anything")] {
        assert!(env_flag_enabled(value), "value should be truthy: {value:?}");
    }
}

#[test]
fn exit_claim_is_single_consumer() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();

    seed_marker(&paths, StartKind::FreshConstruct);

    assert!(matches!(take_exit_claim(&paths), ExitClaim::Claimed { .. }));
    assert_eq!(
        take_exit_claim(&paths),
        ExitClaim::Missing,
        "second exit does not receive a duplicate outro claim"
    );
}

#[test]
fn take_exit_claim_has_exactly_one_winner_under_contention() {
    use std::sync::{Arc, Barrier};

    let tmp = tempfile::tempdir().unwrap();
    let paths = Arc::new(JackinPaths::for_tests(tmp.path()));
    paths.ensure_base_dirs().unwrap();

    let threads = 8;
    // A single 8-thread round catches a non-atomic claim only ~half the
    // time (the threads often don't interleave tightly enough to double-read
    // the marker), so one round is a coin-flip guard. Many rounds drive the
    // miss probability to effectively zero.
    for round in 0..64 {
        seed_marker(&paths, StartKind::FreshConstruct);
        let barrier = Arc::new(Barrier::new(threads));
        // Spawn every thread before joining any; joining in the loop would
        // serialize the race away.
        let mut handles = Vec::with_capacity(threads);
        for _ in 0..threads {
            let paths = Arc::clone(&paths);
            let barrier = Arc::clone(&barrier);
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                matches!(take_exit_claim(&paths), ExitClaim::Claimed { .. })
            }));
        }

        let mut winners = 0;
        for handle in handles {
            if handle.join().unwrap() {
                winners += 1;
            }
        }
        assert_eq!(
            winners, 1,
            "round {round}: exactly one exit may claim the outro"
        );
    }
}

#[test]
fn take_exit_claim_leaves_no_claim_temp_file() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();

    seed_marker(&paths, StartKind::FreshConstruct);
    drop(take_exit_claim(&paths));

    let leftover = std::fs::read_dir(authority(&paths))
        .unwrap()
        .filter_map(Result::ok)
        .any(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("universe-since.claim.")
        });
    assert!(!leftover, "claim temp file must be removed after the take");
}

#[test]
fn malformed_marker_still_grants_exit_claim_without_elapsed() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();

    state_write(&authority(&paths), "universe-since", b"not-a-timestamp").unwrap();

    let ExitClaim::Claimed { elapsed } = take_exit_claim(&paths) else {
        panic!("marker grants close claim");
    };
    assert_eq!(elapsed, None, "malformed marker omits elapsed caption");
    assert!(
        !marker_path(&authority(&paths)).exists(),
        "claim clears malformed marker"
    );
}

#[test]
fn mark_non_fresh_preserves_existing_start() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();

    state_write(&authority(&paths), "universe-since", b"1000").unwrap();
    seed_marker(&paths, StartKind::ResumeExisting); // must not overwrite
    let kept = std::fs::read_to_string(marker_path(&authority(&paths))).unwrap();
    assert_eq!(kept, "1000", "ongoing session keeps its original start");
}

#[tokio::test]
async fn claim_entry_fresh_when_no_running_containers_or_marker() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let docker = FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![]])),
        ..Default::default()
    };

    let claim = claim_entry(&paths, &docker).await;

    assert_eq!(claim.start_kind(), StartKind::FreshConstruct);
    assert!(
        marker_path(&authority(&paths)).exists(),
        "fresh claim writes marker"
    );
    assert!(
        has_pending_claims(&authority(&paths)),
        "fresh claim writes pending file"
    );
}

#[tokio::test]
async fn claim_entry_resumes_when_container_running() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let docker = FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![ContainerRow {
            name: "jk-running".to_owned(),
            id: "container-id".to_owned(),
            labels: HashMap::new(),
        }]])),
        ..Default::default()
    };

    let claim = claim_entry(&paths, &docker).await;

    assert_eq!(claim.start_kind(), StartKind::ResumeExisting);
    assert!(
        marker_path(&authority(&paths)).exists(),
        "resume writes missing marker"
    );
    assert!(
        claim.pending_file.as_ref().unwrap().exists(),
        "joining launch owns pending coverage even when peers currently run"
    );
    // The peer can leave while this joining launch is still preparing.
    let (_, exit) = observe_exit(&paths, &FakeDockerClient::default())
        .await
        .unwrap();
    assert_eq!(exit, ExitClaim::Missing);
    claim.activate().await.unwrap();
    assert_eq!(count_pending_claims(&authority(&paths)), Some(0));
    assert!(marker_path(&authority(&paths)).exists());
}

#[tokio::test]
async fn claim_entry_treats_marker_without_running_containers_as_stale() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    state_write(&authority(&paths), "universe-since", b"1000").unwrap();
    let docker = FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![]])),
        ..Default::default()
    };

    let claim = claim_entry(&paths, &docker).await;

    assert_eq!(claim.start_kind(), StartKind::FreshConstruct);
    let kept = std::fs::read_to_string(marker_path(&authority(&paths))).unwrap();
    assert_ne!(kept, "1000", "stale launch marker is replaced");
}

#[tokio::test]
async fn claim_entry_does_not_write_marker_when_container_list_fails() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let docker = FakeDockerClient {
        fail_with: vec![("docker ps".to_owned(), "daemon down".to_owned())],
        ..Default::default()
    };

    let claim = claim_entry(&paths, &docker).await;

    assert_eq!(claim.start_kind(), StartKind::ResumeExisting);
    assert!(
        !marker_path(&authority(&paths)).exists(),
        "unknown Docker state must not claim the empty construct"
    );
}

#[tokio::test]
async fn release_entry_clears_marker_when_no_instances_or_claims_remain() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let docker = FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![], vec![]])),
        ..Default::default()
    };

    let claim = claim_entry(&paths, &docker).await;
    release_entry_if_idle(&docker, &claim).await;
    drop(claim);

    assert!(
        !marker_path(&authority(&paths)).exists(),
        "idle failed launch clears marker"
    );
    assert!(
        !has_pending_claims(&authority(&paths)),
        "idle failed launch clears pending claim"
    );
}

#[tokio::test]
async fn idle_release_derives_its_root_from_the_owned_pending_file() {
    let first_root = tempfile::tempdir().unwrap();
    let second_root = tempfile::tempdir().unwrap();
    let first_paths = JackinPaths::for_tests(first_root.path());
    let second_paths = JackinPaths::for_tests(second_root.path());
    first_paths.ensure_base_dirs().unwrap();
    second_paths.ensure_base_dirs().unwrap();
    let docker = FakeDockerClient::default();
    let first = claim_entry(&first_paths, &docker).await;
    let second = claim_entry(&second_paths, &docker).await;
    let second_pending = second.pending_file.clone().unwrap();
    let second_marker = std::fs::read_to_string(marker_path(&authority(&second_paths))).unwrap();
    let second_generation = generation(&authority(&second_paths)).unwrap();

    // The release API has no independent paths parameter: callers cannot mix
    // one root's pending owner with another root's lock, generation or marker.
    release_entry_if_idle(&docker, &first).await;
    drop(first);

    assert!(!marker_path(&authority(&first_paths)).exists());
    assert_eq!(count_pending_claims(&authority(&first_paths)), Some(0));
    assert!(second_pending.exists());
    assert_eq!(count_pending_claims(&authority(&second_paths)), Some(1));
    assert_eq!(
        std::fs::read_to_string(marker_path(&authority(&second_paths))).unwrap(),
        second_marker
    );
    assert_eq!(
        generation(&authority(&second_paths)).unwrap(),
        second_generation
    );
}

#[tokio::test]
async fn dropping_entry_removes_only_its_owned_pending_file() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let docker = FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![], vec![]])),
        ..Default::default()
    };

    let first = claim_entry(&paths, &docker).await;
    let first_file = first.pending_file.clone().unwrap();
    let second = claim_entry(&paths, &docker).await;
    let second_file = second.pending_file.clone().unwrap();
    let marker = std::fs::read_to_string(marker_path(&authority(&paths))).unwrap();
    assert_eq!(second.start_kind(), StartKind::ResumeExisting);

    drop(first);

    assert!(
        !first_file.exists(),
        "dropped launch releases its own claim"
    );
    assert!(second_file.exists(), "another launch retains its claim");
    assert_eq!(count_pending_claims(&authority(&paths)), Some(1));
    assert_eq!(
        std::fs::read_to_string(marker_path(&authority(&paths))).unwrap(),
        marker
    );

    drop(second);

    assert_eq!(count_pending_claims(&authority(&paths)), Some(0));
    assert_eq!(
        std::fs::read_to_string(marker_path(&authority(&paths))).unwrap(),
        marker
    );
}

#[tokio::test]
async fn early_launch_errors_do_not_poison_subsequent_entry_claims() {
    async fn failed_launch(paths: &JackinPaths, docker: &impl DockerApi) -> Result<(), ()> {
        let claim = claim_entry(paths, docker).await;
        assert_eq!(claim.start_kind(), StartKind::FreshConstruct);
        assert_eq!(count_pending_claims(&authority(paths)), Some(1));
        Err(())?;
        Ok(())
    }

    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let docker = FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![], vec![], vec![]])),
        ..Default::default()
    };

    for _ in 0..2 {
        assert!(failed_launch(&paths, &docker).await.is_err());
        assert_eq!(count_pending_claims(&authority(&paths)), Some(0));
        assert!(
            marker_path(&authority(&paths)).exists(),
            "drop preserves shared marker"
        );
    }
    let claim = claim_entry(&paths, &docker).await;
    assert_eq!(claim.start_kind(), StartKind::FreshConstruct);
}

#[tokio::test]
async fn dropping_explicitly_released_entry_keeps_a_later_claim() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let docker = FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![], vec![], vec![]])),
        ..Default::default()
    };

    let first = claim_entry(&paths, &docker).await;
    release_entry_if_idle(&docker, &first).await;
    let second = claim_entry(&paths, &docker).await;
    let second_file = second.pending_file.clone().unwrap();
    let marker = std::fs::read_to_string(marker_path(&authority(&paths))).unwrap();

    drop(first);

    assert!(second_file.exists());
    assert_eq!(count_pending_claims(&authority(&paths)), Some(1));
    assert_eq!(
        std::fs::read_to_string(marker_path(&authority(&paths))).unwrap(),
        marker
    );
}

#[tokio::test]
async fn released_entry_cannot_reclaim_a_newer_activated_boundary() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let docker = FakeDockerClient::default();
    let first = claim_entry(&paths, &docker).await;
    release_entry_if_idle(&docker, &first).await;
    assert!(!marker_path(&authority(&paths)).exists());
    let second = claim_entry(&paths, &docker).await;
    second.activate().await.unwrap();
    let marker = std::fs::read_to_string(marker_path(&authority(&paths))).unwrap();
    let current_generation = generation(&authority(&paths)).unwrap();
    let docker_reads = docker.recorded.borrow().len();

    release_entry_if_idle(&docker, &first).await;
    first.activate().await.unwrap();
    second.activate().await.unwrap();
    drop(first);
    drop(second);

    assert_eq!(
        docker.recorded.borrow().len(),
        docker_reads,
        "completed leases cannot acquire a new Docker observation"
    );
    assert_eq!(
        std::fs::read_to_string(marker_path(&authority(&paths))).unwrap(),
        marker
    );
    assert_eq!(generation(&authority(&paths)).unwrap(), current_generation);
}

#[tokio::test]
async fn releasing_entry_preserves_marker_when_docker_state_is_unknown() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let docker = FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![]])),
        ..Default::default()
    };
    let claim = claim_entry(&paths, &docker).await;
    let unavailable = FakeDockerClient {
        fail_with: vec![("docker ps".to_owned(), "daemon down".to_owned())],
        ..Default::default()
    };

    release_entry_if_idle(&unavailable, &claim).await;
    drop(claim);

    assert_eq!(count_pending_claims(&authority(&paths)), Some(0));
    assert!(marker_path(&authority(&paths)).exists());
}

#[tokio::test]
async fn entry_without_pending_file_never_releases_another_launch() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let idle = FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![]])),
        ..Default::default()
    };
    let owner = claim_entry(&paths, &idle).await;
    let unavailable = FakeDockerClient {
        fail_with: vec![("docker ps".to_owned(), "daemon down".to_owned())],
        ..Default::default()
    };
    let unowned = claim_entry(&paths, &unavailable).await;
    assert!(unowned.pending_file.is_none());

    release_entry_if_idle(&idle, &unowned).await;
    drop(unowned);

    assert!(owner.pending_file.as_ref().unwrap().exists());
    assert_eq!(count_pending_claims(&authority(&paths)), Some(1));
    assert!(marker_path(&authority(&paths)).exists());
}

#[tokio::test]
async fn pending_write_failure_keeps_no_token_release_semantics() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    // The claim bails to a lease-less entry when pending recovery is
    // blocked, so the ongoing-session marker comes from the pipeline's
    // separate `mark_start` (mirrored here); release must still preserve
    // both the marker and the blocking file.
    seed_marker(&paths, StartKind::FreshConstruct);
    std::fs::write(pending_dir(&authority(&paths)), "blocked directory").unwrap();
    let docker = FakeDockerClient::default();

    let claim = claim_entry(&paths, &docker).await;
    assert_eq!(claim.start_kind(), StartKind::ResumeExisting);
    assert!(claim.pending_file.is_none());
    release_entry_if_idle(&docker, &claim).await;
    drop(claim);

    assert!(marker_path(&authority(&paths)).exists());
    assert_eq!(
        std::fs::read_to_string(pending_dir(&authority(&paths))).unwrap(),
        "blocked directory"
    );
}

#[tokio::test]
async fn generation_failure_prevents_destructive_cleanup() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let docker = FakeDockerClient::default();
    let claim = claim_entry(&paths, &docker).await;
    let pending_file = claim.pending_file.clone().unwrap();
    std::fs::remove_file(authority(&paths).join("universe-generation")).unwrap();
    std::fs::create_dir(authority(&paths).join("universe-generation")).unwrap();

    release_entry_if_idle(&docker, &claim).await;
    drop(claim);
    assert_eq!(take_exit_claim(&paths), ExitClaim::Missing);

    assert!(
        pending_file.exists(),
        "untracked mutation must remain conservatively pending"
    );
    assert!(marker_path(&authority(&paths)).exists());
}

#[tokio::test]
async fn release_entry_keeps_marker_when_another_claim_remains() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let docker = FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([
            vec![],
            vec![],
            vec![],
            vec![],
        ])),
        ..Default::default()
    };

    let first = claim_entry(&paths, &docker).await;
    let second = claim_entry(&paths, &docker).await;
    release_entry_if_idle(&docker, &first).await;

    assert!(
        marker_path(&authority(&paths)).exists(),
        "another pending launch keeps construct marker"
    );

    release_entry_if_idle(&docker, &second).await;

    assert!(
        !marker_path(&authority(&paths)).exists(),
        "last pending launch clears marker"
    );
}
