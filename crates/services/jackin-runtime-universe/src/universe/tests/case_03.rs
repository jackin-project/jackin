// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
    let live_lease =
        coordination::open_state_in_namespace(&pending_dir(&directory), pending_key, false)
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
        entrant.pending_file().unwrap().exists(),
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
                    "universe::tests::case_03::entry_claim_process_worker",
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
