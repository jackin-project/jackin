// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
        coordination::open_state_in_namespace(&pending_dir(&directory), key, false).unwrap();
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
        coordination::open_state_in_namespace(&pending_dir(&directory), key, false).unwrap();
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
