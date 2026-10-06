// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
