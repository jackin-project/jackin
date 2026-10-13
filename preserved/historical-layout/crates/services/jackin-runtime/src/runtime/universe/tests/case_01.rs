// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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
    coordination::ensure_prunable(&paths, &paths.jackin_home).unwrap();

    std::fs::remove_dir_all(&paths.jackin_home).unwrap();
    paths.ensure_base_dirs().unwrap();

    assert_eq!(authority(&paths), directory);
    let contender = coordination::open_in_namespace(&directory, "universe-lock").unwrap();
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
    coordination::ensure_prunable(&paths, &paths.data_dir).unwrap();

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
