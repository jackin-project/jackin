// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::leader::{read_lease, write_lease};
use nix::fcntl::{FcntlArg, FdFlag, fcntl};

#[test]
fn usage_broker_recovers_stale_guard_with_private_permissions() {
    let temp = tempfile::tempdir().unwrap();
    let config = UsageBrokerConfig::for_data_dir(temp.path().to_owned());
    let run_dir = secure_run_directory(&config.data_dir).unwrap();
    let leader = run_dir.join(BROKER_LEADER);
    fs::write(&leader, "2147483647\n").unwrap();
    fs::set_permissions(&leader, fs::Permissions::from_mode(0o600)).unwrap();
    let executor = Arc::new(CountingExecutor {
        calls: AtomicUsize::new(0),
    });

    let client = ensure_usage_broker_with_executor(config.clone(), executor).unwrap();
    assert!(connect_probe(&client));
    assert_eq!(fs::metadata(run_dir).unwrap().mode() & 0o777, 0o700);
    assert_eq!(
        fs::metadata(config.socket_path()).unwrap().mode() & 0o777,
        0o600
    );
    assert_eq!(fs::metadata(leader).unwrap().mode() & 0o777, 0o600);
}

#[test]
fn broker_startup_failure_cleans_lease_and_socket_before_returning() {
    let temp = tempfile::tempdir().unwrap();
    let config = UsageBrokerConfig::for_data_dir(temp.path().to_owned());
    let projection = temp.path().join(BROKER_DIR).join("projection.json");
    fs::create_dir_all(projection.parent().unwrap()).unwrap();
    fs::create_dir(&projection).unwrap();

    let error = ensure_usage_broker_with_executor(
        config.clone(),
        Arc::new(CountingExecutor {
            calls: AtomicUsize::new(0),
        }),
    )
    .unwrap_err();
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unavailable);

    let run_dir = temp.path().join(BROKER_DIR).join(BROKER_RUN_DIR);
    assert!(!run_dir.join(BROKER_LEADER).exists());
    assert!(!config.socket_path().exists());
}

#[test]
fn broker_lease_uses_expiry_and_build_identity_not_pid_reuse() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("lease");
    let mut live = BrokerLease::new("build");
    fs::write(&path, serde_json::to_vec(&live).unwrap()).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(
        claim_leader(&path, "build", Duration::from_secs(30))
            .unwrap()
            .is_none()
    );

    live.renewed_at_epoch -= 31;
    fs::write(&path, serde_json::to_vec(&live).unwrap()).unwrap();
    let replacement = claim_leader(&path, "build", Duration::from_secs(30))
        .unwrap()
        .expect("expired lease is reclaimable");
    assert_ne!(replacement.lease.instance_id, live.instance_id);

    fs::write(&path, serde_json::to_vec(&replacement.lease).unwrap()).unwrap();
    drop(replacement);
    assert!(
        claim_leader(&path, "other-build", Duration::from_secs(30))
            .unwrap()
            .is_none()
    );
}

#[test]
fn lease_descriptors_are_close_on_exec_for_new_and_recovered_claims() {
    let temp = tempfile::tempdir().unwrap();
    let lease_path = temp.path().join("lease");
    let mut initial = claim_leader(&lease_path, "build", Duration::from_secs(30))
        .unwrap()
        .expect("first claimant owns the new lease");
    assert_close_on_exec(&initial.file);

    let mut expired = initial.lease.clone();
    expired.renewed_at_epoch -= 31;
    write_lease(&mut initial.file, &expired).unwrap();
    initial.file.unlock().unwrap();
    drop(initial);

    let recovered = claim_leader(&lease_path, "build", Duration::from_secs(30))
        .unwrap()
        .expect("expired lease is recoverable after its owner lock is released");
    assert_close_on_exec(&recovered.file);
}

fn assert_close_on_exec(file: &fs::File) {
    let flags = fcntl(file, FcntlArg::F_GETFD).unwrap();
    assert!(FdFlag::from_bits_truncate(flags).contains(FdFlag::FD_CLOEXEC));
}

#[test]
fn expired_current_owner_can_renew_with_a_fake_clock() {
    let temp = tempfile::tempdir().unwrap();
    let lease_path = temp.path().join("lease");
    let mut owner = claim_leader(&lease_path, "build", Duration::from_secs(30))
        .unwrap()
        .expect("first claimant owns the lease");
    let original = owner.lease.clone();
    let lease_duration = Duration::from_secs(30);
    owner.lease.renewed_at_epoch -= i64::try_from(lease_duration.as_secs()).unwrap() + 1;
    write_lease(&mut owner.file, &owner.lease).unwrap();
    let now_epoch =
        owner.lease.renewed_at_epoch + i64::try_from(lease_duration.as_secs()).unwrap() + 1;

    assert!(leader::renew_lease_at(&mut owner, now_epoch));
    assert_eq!(owner.lease.instance_id, original.instance_id);
    assert_eq!(owner.lease.process_id, std::process::id());
    assert_eq!(owner.lease.renewed_at_epoch, now_epoch);

    let persisted = read_lease(&mut owner.file).unwrap();
    assert_eq!(persisted.instance_id, original.instance_id);
    assert_eq!(persisted.renewed_at_epoch, now_epoch);
}

#[test]
fn expired_live_owner_lock_prevents_successor_takeover() {
    let temp = tempfile::tempdir().unwrap();
    let lease_path = temp.path().join("lease");
    let mut owner = claim_leader(&lease_path, "build", Duration::from_secs(30))
        .unwrap()
        .expect("first claimant owns the lease");
    owner.lease.renewed_at_epoch -= 31;
    write_lease(&mut owner.file, &owner.lease).unwrap();
    let owner_id = owner.lease.instance_id.clone();

    assert!(
        claim_leader(&lease_path, "build", Duration::from_secs(30))
            .unwrap()
            .is_none(),
        "the OS-held owner lock fences takeover even after wall-clock expiry"
    );
    let current = read_lease(&mut owner.file).unwrap();
    assert_eq!(current.instance_id, owner_id);
    assert_eq!(current.renewed_at_epoch, owner.lease.renewed_at_epoch);
}

#[test]
fn stale_lease_descriptor_cannot_renew_or_clean_successor_files() {
    let temp = tempfile::tempdir().unwrap();
    let lease_path = temp.path().join("lease");
    let socket_path = temp.path().join("socket");
    let mut stale = claim_leader(&lease_path, "build", Duration::from_secs(30))
        .unwrap()
        .expect("first claimant owns the lease");
    fs::write(&socket_path, b"successor socket").unwrap();

    fs::remove_file(&lease_path).unwrap();
    let successor = claim_leader(&lease_path, "build", Duration::from_secs(30))
        .unwrap()
        .expect("successor owns the replacement lease");
    let successor_id = successor.lease.instance_id.clone();

    assert!(!renew_lease(&mut stale));
    assert!(!cleanup_owned_files(&lease_path, &socket_path, &mut stale,));
    let current: BrokerLease = serde_json::from_slice(&fs::read(&lease_path).unwrap()).unwrap();
    assert_eq!(current.instance_id, successor_id);
    assert!(socket_path.exists());
}

#[test]
fn expired_successor_lease_fences_old_owner_renewal_and_cleanup() {
    let temp = tempfile::tempdir().unwrap();
    let lease_path = temp.path().join("lease");
    let socket_path = temp.path().join("socket");
    let mut old_owner = claim_leader(&lease_path, "build", Duration::from_secs(30))
        .unwrap()
        .expect("first claimant owns the lease");
    fs::write(&socket_path, b"successor socket").unwrap();

    let mut expired = old_owner.lease.clone();
    expired.renewed_at_epoch -= 31;
    write_lease(&mut old_owner.file, &expired).unwrap();
    // Simulate process death: the open descriptor remains for this stale-owner
    // assertion, but the OS releases its lifetime lock before succession.
    old_owner.file.unlock().unwrap();
    let mut successor = claim_leader(&lease_path, "build", Duration::from_secs(30))
        .unwrap()
        .expect("successor claims the expired lease on the same inode");
    let successor_id = successor.lease.instance_id.clone();
    assert_ne!(successor_id, old_owner.lease.instance_id);

    assert!(!leader::renew_lease_at(
        &mut old_owner,
        successor.lease.renewed_at_epoch + 31,
    ));
    assert!(!cleanup_owned_files(
        &lease_path,
        &socket_path,
        &mut old_owner,
    ));

    let current = read_lease(&mut successor.file).unwrap();
    assert_eq!(current.instance_id, successor_id);
    assert!(socket_path.exists());
}

#[test]
fn usage_broker_rejects_symlinked_run_tree_without_mutating_target() {
    let temp = tempfile::tempdir().unwrap();
    let data_dir = temp.path().join("data");
    let target = temp.path().join("target");
    fs::create_dir(&data_dir).unwrap();
    fs::create_dir(&target).unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o755)).unwrap();
    symlink(&target, data_dir.join(BROKER_DIR)).unwrap();
    let executor: Arc<dyn UsageProviderExecutor> = Arc::new(CountingExecutor {
        calls: AtomicUsize::new(0),
    });

    let result =
        ensure_usage_broker_with_executor(UsageBrokerConfig::for_data_dir(data_dir), executor);
    result.unwrap_err();
    assert_eq!(fs::metadata(target).unwrap().mode() & 0o777, 0o755);
}

#[test]
fn saturated_join_waiters_do_not_block_refresh_or_current() {
    let temp = tempfile::tempdir().unwrap();
    let (started_tx, started_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let executor: Arc<dyn UsageProviderExecutor> = Arc::new(HeldExecutor {
        started: started_tx,
        release: Mutex::new(release_rx),
    });
    let config = UsageBrokerConfig::for_data_dir(temp.path().to_owned());
    let client = ensure_usage_broker_with_executor(config.clone(), executor).unwrap();
    let active = client.refresh(capability(), 0, true).unwrap();
    started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let mut waiters = Vec::new();
    for _ in 0..BROKER_CONNECTION_WORKERS * 2 {
        let mut stream = UnixStream::connect(config.socket_path()).unwrap();
        let request = UsageBrokerRequest {
            protocol_version: USAGE_BROKER_PROTOCOL_VERSION.to_owned(),
            build_id: config.build_id.clone(),
            operation: UsageBrokerOperation::Join {
                capability: capability(),
                generation: active.generation,
                timeout_ms: 10_000,
            },
            launch_credential_scope: None,
        };
        let mut bytes = serde_json::to_vec(&request).unwrap();
        bytes.push(b'\n');
        stream.write_all(&bytes).unwrap();
        waiters.push(stream);
    }
    let (response_tx, response_rx) = mpsc::sync_channel(1);
    let control = client.clone();
    let request = thread::spawn(move || {
        let started = Instant::now();
        let short_wait = control.join(capability(), active.generation, Duration::from_millis(1));
        let elapsed = started.elapsed();
        let result = control
            .refresh(capability(), 0, true)
            .and_then(|_| control.current(capability()));
        response_tx.send((short_wait, elapsed, result)).unwrap();
    });
    let response = response_rx.recv_timeout(Duration::from_secs(2));
    // Always release the provider before asserting, so a failed regression
    // cannot strand fixture threads or turn cleanup into another timeout.
    release_tx.send(()).unwrap();
    request.join().unwrap();
    let (short_wait, elapsed, response) =
        response.expect("long polls starved a short wait or control requests");
    assert_eq!(
        short_wait.unwrap_err().kind,
        UsageCoordinationErrorKind::WaitTimeout
    );
    assert!(
        elapsed < Duration::from_secs(1),
        "short join queued behind unrelated long polls"
    );
    let response = response.unwrap();
    assert_eq!(response.generation, active.generation);
    assert!(response.phase.is_active());
    for mut waiter in waiters {
        let response: UsageBrokerResponse = read_frame(&mut waiter).unwrap();
        assert!(
            matches!(response, UsageBrokerResponse::State { state } if state.phase == UsageRefreshPhase::Completed)
        );
    }
}

#[test]
fn stalled_response_reader_does_not_hold_worker_shutdown() {
    let (mut server, client) = UnixStream::pair().unwrap();
    let (done_tx, done_rx) = mpsc::sync_channel(1);
    let worker = thread::spawn(move || {
        let bytes = vec![b'x'; 8 * 1024 * 1024];
        write_with_deadline(&mut server, &bytes, Duration::from_millis(50));
        done_tx.send(()).unwrap();
    });
    let finished = done_rx.recv_timeout(Duration::from_secs(1));
    // Even the failing implementation can be joined once the peer closes.
    drop(client);
    worker.join().unwrap();
    assert!(
        finished.is_ok(),
        "stalled reader prevented bounded worker shutdown"
    );
}

#[test]
fn subscribe_all_dedups_reuses_fresh_and_forces_only_on_demand() {
    let temp = tempfile::tempdir().unwrap();
    let executor = Arc::new(CountingExecutor {
        calls: AtomicUsize::new(0),
    });
    let concrete_executor = Arc::clone(&executor);
    let broker_executor: Arc<dyn UsageProviderExecutor> = concrete_executor;
    let client = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().to_owned()),
        broker_executor,
    )
    .unwrap();

    // Due-on-open with a duplicated capability issues one request per account.
    let opened = client.subscribe_all([capability(), second_capability(), capability()]);
    assert_eq!(opened.len(), 2);
    assert!(opened.iter().all(|(_, result)| result.is_ok()));
    assert_eq!(
        client.subscriptions(),
        vec![capability(), second_capability()]
    );
    for (_, result) in &opened {
        let view = result.as_ref().unwrap();
        client
            .join(
                view.capability.clone(),
                view.generation,
                Duration::from_secs(5),
            )
            .unwrap();
    }
    assert_eq!(executor.calls.load(Ordering::SeqCst), 2);

    // Still-fresh observations are reused; nothing new is forced.
    let reopened = client.subscribe_all([capability(), second_capability()]);
    assert!(reopened.iter().all(|(_, result)| result.is_ok()));
    let heartbeat = client.refresh_due(false);
    assert_eq!(heartbeat.len(), 2);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 2);

    // An explicit operator refresh bypasses Codex's success cooldown, while
    // Claude's persisted minimum attempt interval still applies.
    let forced = client.refresh_due(true);
    assert!(forced.iter().all(|(_, result)| result.is_ok()));
    for (_, result) in &forced {
        let view = result.as_ref().unwrap();
        let expected_generation = if view.capability == capability() {
            1
        } else {
            2
        };
        assert_eq!(view.generation, expected_generation);
        client
            .join(
                view.capability.clone(),
                view.generation,
                Duration::from_secs(5),
            )
            .unwrap();
    }
    assert_eq!(executor.calls.load(Ordering::SeqCst), 3);
}

#[test]
fn unsubscribe_releases_local_interest_without_cancelling_shared_work() {
    let temp = tempfile::tempdir().unwrap();
    let (started_tx, started_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let executor: Arc<dyn UsageProviderExecutor> = Arc::new(HeldExecutor {
        started: started_tx,
        release: Mutex::new(release_rx),
    });
    let client = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().to_owned()),
        executor,
    )
    .unwrap();

    let opened = client.subscribe(capability()).unwrap();
    started_rx.recv_timeout(Duration::from_secs(5)).unwrap();

    // Prompt unsubscribe performs no broker I/O and leaves the broker-owned
    // generation untouched.
    assert!(client.unsubscribe(&capability()));
    assert!(!client.unsubscribe(&capability()));
    assert!(client.subscriptions().is_empty());
    let active = client.current(capability()).unwrap();
    assert_eq!(active.generation, opened.generation);
    assert!(active.phase.is_active());

    // Another client awaiting the same generation still observes terminal.
    release_tx.send(()).unwrap();
    let waiter = client.clone();
    let terminal = waiter
        .join(capability(), opened.generation, Duration::from_secs(5))
        .unwrap();
    assert_eq!(terminal.phase, UsageRefreshPhase::Completed);
    assert!(terminal.snapshot.is_some());
}

#[test]
fn client_clone_forks_subscription_set() {
    let temp = tempfile::tempdir().unwrap();
    let executor: Arc<dyn UsageProviderExecutor> = Arc::new(CountingExecutor {
        calls: AtomicUsize::new(0),
    });
    let client = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().to_owned()),
        executor,
    )
    .unwrap();
    client.subscribe(capability()).unwrap();
    let fork = client.clone();

    assert!(fork.unsubscribe(&capability()));
    assert_eq!(fork.subscriptions(), Vec::new());
    assert_eq!(client.subscriptions(), vec![capability()]);
    assert_eq!(client.observed_generation(&capability()), Some(1));

    client.unsubscribe_all();
    assert!(client.subscriptions().is_empty());
}
