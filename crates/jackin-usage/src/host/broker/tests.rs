// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::os::unix::fs::symlink;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;

use jackin_protocol::control::{
    FocusedUsageView, QuotaBucketView, UsageConfidence, UsageSeverity, UsageSnapshotStatus,
    UsageSource,
};
use jackin_protocol::usage_broker::UsageRefreshPhase;

use super::*;

struct CountingExecutor {
    calls: AtomicUsize,
}

impl UsageProviderExecutor for CountingExecutor {
    fn probe(
        &self,
        _capability: &UsageAccountCapability,
        _generation: u64,
    ) -> ProviderProbeOutcome {
        self.calls.fetch_add(1, Ordering::SeqCst);
        ProviderProbeOutcome::success(quota_view())
    }
}

fn capability() -> UsageAccountCapability {
    UsageAccountCapability {
        account_id: "abc123".to_owned(),
        surface_id: "claude".to_owned(),
    }
}

fn quota_view() -> FocusedUsageView {
    let mut view = FocusedUsageView::unavailable("claude", chrono::Utc::now().timestamp());
    view.status = UsageSnapshotStatus::Fresh;
    view.source = UsageSource::ProviderApi;
    view.confidence = UsageConfidence::Authoritative;
    view.account.provider_label = "Claude".to_owned();
    view.account.account_label = "account@example.test".to_owned();
    view.buckets = vec![QuotaBucketView {
        label: "Weekly".to_owned(),
        used_label: None,
        limit_label: None,
        remaining_percent: Some(75),
        reset_label: None,
        resets_at: None,
        status_slot: None,
        pace_label: None,
        status: UsageSnapshotStatus::Fresh,
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::Normal,
    }];
    view
}

#[test]
fn discovery_provider_rate_limit_preserves_retry_after() {
    let before = chrono::Utc::now().timestamp();
    let mut view = quota_view();
    view.status = UsageSnapshotStatus::Stale;
    view.last_error = Some("provider HTTP 429; Retry-After: 97".to_owned());

    let ProviderProbeOutcome::Failure {
        kind,
        message,
        retry_at_epoch,
    } = provider_probe_outcome(view)
    else {
        panic!("rate-limited view must not publish as success");
    };
    let after = chrono::Utc::now().timestamp();
    assert_eq!(kind, UsageCoordinationErrorKind::RateLimited);
    assert_eq!(message, "usage provider rate limit is active");
    assert!(
        retry_at_epoch.is_some_and(|deadline| { (before + 97..=after + 97).contains(&deadline) })
    );
}

#[test]
fn forwarded_scope_selects_only_accounts_backed_by_forwarded_sources() {
    use crate::host::{CanonicalAccountIdentity, CanonicalAccountSubject, HostSurfaceId};

    let profile_identity = CanonicalAccountIdentity {
        surface: HostSurfaceId::Amp,
        subject: CanonicalAccountSubject::ProviderStableHandle("profile@example.test".to_owned()),
    };
    let env_identity = CanonicalAccountIdentity {
        surface: HostSurfaceId::Amp,
        subject: CanonicalAccountSubject::ProviderStableHandle("env@example.test".to_owned()),
    };
    let scope = "workspace sample role test";
    let discovery = ValidatedUsageDiscovery {
        config_generation: Some("generation".to_owned()),
        accounts: Vec::new(),
        diagnostics: Vec::new(),
        candidates: Vec::new(),
        bindings: vec![
            ValidatedCredentialBinding {
                surface: HostSurfaceId::Amp,
                identity: Some(profile_identity),
                source_id: "profile-source".to_owned(),
                capability_id: "profile-capability".to_owned(),
                provenance: std::collections::BTreeSet::from([scope.to_owned()]),
                source: ValidatedCredentialSource::Profile(
                    super::super::discovery::ProfileCredentialMaterial::Amp {
                        key: "profile-secret".to_owned(),
                    },
                ),
            },
            ValidatedCredentialBinding {
                surface: HostSurfaceId::Amp,
                identity: Some(env_identity),
                source_id: "env-source".to_owned(),
                capability_id: "env-capability".to_owned(),
                provenance: std::collections::BTreeSet::from([scope.to_owned()]),
                source: ValidatedCredentialSource::Env {
                    handle: super::super::OpaqueCredentialHandle::new("env-handle"),
                    key: "AMP_API_KEY".to_owned(),
                },
            },
        ],
    };
    let profile_capability = capability_for_binding(&discovery.bindings[0]);
    let env_capability = capability_for_binding(&discovery.bindings[1]);

    let profile_only = forwarded_usage_capabilities(
        &discovery,
        scope,
        &ForwardedUsageSources {
            profile_surface_ids: std::collections::BTreeSet::from(["amp".to_owned()]),
            env_keys: std::collections::BTreeSet::new(),
        },
    );
    assert_eq!(profile_only, vec![profile_capability]);

    let env_only = forwarded_usage_capabilities(
        &discovery,
        scope,
        &ForwardedUsageSources {
            profile_surface_ids: std::collections::BTreeSet::new(),
            env_keys: std::collections::BTreeSet::from(["AMP_API_KEY".to_owned()]),
        },
    );
    assert_eq!(env_only, vec![env_capability]);
}

#[test]
fn usage_broker_twenty_clients_join_one_generation_and_probe() {
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
    let barrier = Arc::new(Barrier::new(20));
    let mut clients = Vec::new();
    for _ in 0..20 {
        let client = client.clone();
        let barrier = Arc::clone(&barrier);
        clients.push(thread::spawn(move || {
            barrier.wait();
            client.refresh(capability(), 0, true).unwrap()
        }));
    }
    let generations = clients
        .into_iter()
        .map(|client| client.join().unwrap())
        .collect::<Vec<_>>();
    assert!(generations.iter().all(|state| state.generation == 1));
    let terminal = client
        .join(capability(), 1, Duration::from_secs(2))
        .unwrap();
    assert_eq!(terminal.phase, UsageRefreshPhase::Completed);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn usage_broker_handshake_mismatch_fails_before_provider_dispatch() {
    let temp = tempfile::tempdir().unwrap();
    let executor = Arc::new(CountingExecutor {
        calls: AtomicUsize::new(0),
    });
    let config = UsageBrokerConfig::for_data_dir(temp.path().to_owned());
    let concrete_executor = Arc::clone(&executor);
    let broker_executor: Arc<dyn UsageProviderExecutor> = concrete_executor;
    let client = ensure_usage_broker_with_executor(config.clone(), broker_executor).unwrap();
    let incompatible = UsageBrokerClient::at(client.socket_path, "other-build".to_owned());
    let error = incompatible.refresh(capability(), 0, true).unwrap_err();
    assert_eq!(error.kind, UsageCoordinationErrorKind::ProtocolMismatch);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
}

#[test]
fn broker_client_scoped_operation_requires_relay_and_never_probes() {
    let temp = tempfile::tempdir().unwrap();
    let executor = Arc::new(CountingExecutor {
        calls: AtomicUsize::new(0),
    });
    let concrete = Arc::clone(&executor);
    let broker_executor: Arc<dyn UsageProviderExecutor> = concrete;
    let client = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().to_owned()),
        broker_executor,
    )
    .unwrap();

    let error = client.current_for_surface("claude").unwrap_err();
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unauthorized);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
}

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
    assert_ne!(replacement.instance_id, live.instance_id);

    fs::write(&path, serde_json::to_vec(&replacement).unwrap()).unwrap();
    assert!(
        claim_leader(&path, "other-build", Duration::from_secs(30))
            .unwrap()
            .is_none()
    );
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

struct HeldExecutor {
    started: mpsc::SyncSender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}

impl UsageProviderExecutor for HeldExecutor {
    fn probe(&self, _: &UsageAccountCapability, _: u64) -> ProviderProbeOutcome {
        self.started.send(()).unwrap();
        self.release
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(10))
            .unwrap();
        ProviderProbeOutcome::success(quota_view())
    }
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
