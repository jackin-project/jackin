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
use jackin_docker::docker_client::ContainerRow;
use jackin_test_support::FakeDockerClient;
use std::collections::{HashMap, VecDeque};

#[test]
fn mark_then_take_round_trips_and_clears() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();

    mark_start(&paths, StartKind::FreshConstruct);
    assert!(marker_path(&paths).exists(), "marker written");

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
    assert!(!marker_path(&paths).exists(), "marker cleared after take");
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

    mark_start(&paths, StartKind::FreshConstruct);

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
        mark_start(&paths, StartKind::FreshConstruct);
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

    mark_start(&paths, StartKind::FreshConstruct);
    drop(take_exit_claim(&paths));

    let leftover = std::fs::read_dir(&paths.data_dir)
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

    std::fs::write(marker_path(&paths), "not-a-timestamp").unwrap();

    let ExitClaim::Claimed { elapsed } = take_exit_claim(&paths) else {
        panic!("marker grants close claim");
    };
    assert_eq!(elapsed, None, "malformed marker omits elapsed caption");
    assert!(
        !marker_path(&paths).exists(),
        "claim clears malformed marker"
    );
}

#[test]
fn mark_non_fresh_preserves_existing_start() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();

    std::fs::write(marker_path(&paths), "1000").unwrap();
    mark_start(&paths, StartKind::ResumeExisting); // must not overwrite
    let kept = std::fs::read_to_string(marker_path(&paths)).unwrap();
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
    assert!(marker_path(&paths).exists(), "fresh claim writes marker");
    assert!(
        has_pending_claims(&paths),
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
            labels: HashMap::new(),
        }]])),
        ..Default::default()
    };

    let claim = claim_entry(&paths, &docker).await;

    assert_eq!(claim.start_kind(), StartKind::ResumeExisting);
    assert!(marker_path(&paths).exists(), "resume writes missing marker");
}

#[tokio::test]
async fn claim_entry_treats_marker_without_running_containers_as_stale() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    std::fs::write(marker_path(&paths), "1000").unwrap();
    let docker = FakeDockerClient {
        list_containers_queue: std::cell::RefCell::new(VecDeque::from([vec![]])),
        ..Default::default()
    };

    let claim = claim_entry(&paths, &docker).await;

    assert_eq!(claim.start_kind(), StartKind::FreshConstruct);
    let kept = std::fs::read_to_string(marker_path(&paths)).unwrap();
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
        !marker_path(&paths).exists(),
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
    release_entry_if_idle(&paths, &docker, &claim).await;

    assert!(
        !marker_path(&paths).exists(),
        "idle failed launch clears marker"
    );
    assert!(
        !has_pending_claims(&paths),
        "idle failed launch clears pending claim"
    );
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
    release_entry_if_idle(&paths, &docker, &first).await;

    assert!(
        marker_path(&paths).exists(),
        "another pending launch keeps construct marker"
    );

    release_entry_if_idle(&paths, &docker, &second).await;

    assert!(
        !marker_path(&paths).exists(),
        "last pending launch clears marker"
    );
}
