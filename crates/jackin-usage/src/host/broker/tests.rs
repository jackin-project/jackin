// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};
use std::os::unix::fs::symlink;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;

use jackin_protocol::control::{
    FocusedUsageView, QuotaBucketView, UsageConfidence, UsageSeverity, UsageSnapshotStatus,
    UsageSource,
};
use jackin_protocol::usage_broker::{
    UsageCatalogEntry, UsageFreshnessPhaseV1, UsageIdentityKindV1, UsageProjectionRefreshStateV1,
    UsageRefreshPhase,
};

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

fn second_capability() -> UsageAccountCapability {
    UsageAccountCapability {
        account_id: "def456".to_owned(),
        surface_id: "codex".to_owned(),
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
fn discovery_provider_stale_and_error_views_are_retryable_failures() {
    for status in [UsageSnapshotStatus::Stale, UsageSnapshotStatus::Error] {
        let mut view = quota_view();
        view.status = status;
        view.last_error = None;
        let ProviderProbeOutcome::Failure {
            kind,
            retry_at_epoch,
            ..
        } = provider_probe_outcome(view)
        else {
            panic!("{status:?} provider view must not publish as success");
        };
        assert_eq!(kind, UsageCoordinationErrorKind::ProviderUnavailable);
        assert_eq!(retry_at_epoch, None);
    }
}

#[test]
fn discovery_provider_unsupported_views_remain_publishable_unsupported() {
    let mut view = quota_view();
    view.status = UsageSnapshotStatus::Unsupported;
    assert!(matches!(
        provider_probe_outcome(view),
        ProviderProbeOutcome::Success(_)
    ));
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
                provenance: BTreeSet::from([
                    scope.to_owned(),
                    "account account-profile".to_owned(),
                ]),
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
                provenance: BTreeSet::from([scope.to_owned(), "account account-env".to_owned()]),
                source: ValidatedCredentialSource::Env {
                    handle: super::super::OpaqueCredentialHandle::new("env-handle"),
                    key: "AMP_API_KEY".to_owned(),
                },
            },
        ],
    };
    let profile_capability = capability_for_binding(
        &discovery.bindings[0],
        discovery.config_generation.as_deref(),
    );
    let env_capability = capability_for_binding(
        &discovery.bindings[1],
        discovery.config_generation.as_deref(),
    );

    let profile_only = forwarded_usage_capabilities(
        &discovery,
        scope,
        &ForwardedUsageSources {
            selected_account_ids: BTreeSet::new(),
            selected_account_surfaces: BTreeMap::new(),
            profile_surface_ids: BTreeSet::from(["amp".to_owned()]),
            env_keys: BTreeSet::new(),
        },
    );
    assert_eq!(profile_only, vec![profile_capability.clone()]);

    let env_only = forwarded_usage_capabilities(
        &discovery,
        scope,
        &ForwardedUsageSources {
            selected_account_ids: BTreeSet::new(),
            selected_account_surfaces: BTreeMap::new(),
            profile_surface_ids: BTreeSet::new(),
            env_keys: BTreeSet::from(["AMP_API_KEY".to_owned()]),
        },
    );
    assert_eq!(env_only, vec![env_capability.clone()]);

    let selected_profile = forwarded_usage_capabilities(
        &discovery,
        scope,
        &ForwardedUsageSources {
            selected_account_ids: BTreeSet::from(["account-profile".to_owned()]),
            selected_account_surfaces: BTreeMap::from([(
                "account-profile".to_owned(),
                "amp".to_owned(),
            )]),
            profile_surface_ids: BTreeSet::from(["amp".to_owned()]),
            env_keys: BTreeSet::new(),
        },
    );
    assert_eq!(selected_profile, vec![profile_capability.clone()]);

    let selected_env = forwarded_usage_capabilities(
        &discovery,
        scope,
        &ForwardedUsageSources {
            selected_account_ids: BTreeSet::from(["account-env".to_owned()]),
            selected_account_surfaces: BTreeMap::from([(
                "account-env".to_owned(),
                "amp".to_owned(),
            )]),
            profile_surface_ids: BTreeSet::new(),
            env_keys: BTreeSet::from(["AMP_API_KEY".to_owned()]),
        },
    );
    assert_eq!(selected_env, vec![env_capability]);

    let wrong_account = forwarded_usage_capabilities(
        &discovery,
        scope,
        &ForwardedUsageSources {
            selected_account_ids: BTreeSet::from(["account-does-not-exist".to_owned()]),
            selected_account_surfaces: BTreeMap::new(),
            profile_surface_ids: BTreeSet::from(["amp".to_owned()]),
            env_keys: BTreeSet::from(["AMP_API_KEY".to_owned()]),
        },
    );
    assert!(wrong_account.is_empty());

    assert_eq!(
        usage_capability_for_selected_account(&discovery, "account-profile", "amp"),
        Some(profile_capability.clone())
    );
    assert_eq!(
        usage_capability_for_selected_account(&discovery, "account-profile", "claude"),
        None
    );

    let publication = publication_identity_metadata(&discovery);
    assert_eq!(
        publication[&profile_capability].identity_kind,
        UsageIdentityKindV1::ProviderStableHandle
    );
    assert_eq!(publication[&profile_capability].provenance_count, 2);
}

#[test]
fn rotated_catalog_revision_rejects_in_flight_broker_result() {
    use crate::host::{CanonicalAccountIdentity, CanonicalAccountSubject, HostSurfaceId};

    let binding = ValidatedCredentialBinding {
        surface: HostSurfaceId::Claude,
        identity: Some(CanonicalAccountIdentity {
            surface: HostSurfaceId::Claude,
            subject: CanonicalAccountSubject::ProviderId("provider-account".to_owned()),
        }),
        source_id: "source-0001".to_owned(),
        capability_id: "capability-0001".to_owned(),
        provenance: BTreeSet::from(["account work".to_owned()]),
        source: ValidatedCredentialSource::Capability,
    };
    let old_capability = capability_for_binding(&binding, Some("generation-old"));
    let current_capability = capability_for_binding(&binding, Some("generation-current"));
    assert_ne!(old_capability, current_capability);

    let temp = tempfile::tempdir().unwrap();
    let mut runtime = HostUsageRuntime::new();
    runtime
        .open(crate::host::HostRuntimeConfig::under_data_dir(temp.path()))
        .unwrap();
    runtime.discovery = Some(ValidatedUsageDiscovery {
        config_generation: Some("generation-current".to_owned()),
        accounts: Vec::new(),
        diagnostics: Vec::new(),
        candidates: Vec::new(),
        bindings: vec![binding],
    });

    runtime
        .apply_broker_generation(UsageGenerationView {
            capability: old_capability,
            generation: 1,
            phase: UsageRefreshPhase::Completed,
            snapshot: Some(quota_view()),
            error: None,
            retry_at_epoch: None,
        })
        .unwrap();

    assert!(runtime.discovered_views.is_empty());
    assert!(runtime.discovered_provider_views.is_empty());
}

#[test]
fn broker_catalog_admits_current_identity_and_rejects_stale_identity() {
    use crate::host::{CanonicalAccountIdentity, CanonicalAccountSubject, HostSurfaceId};

    let binding = ValidatedCredentialBinding {
        surface: HostSurfaceId::Claude,
        identity: Some(CanonicalAccountIdentity {
            surface: HostSurfaceId::Claude,
            subject: CanonicalAccountSubject::ProviderId("provider-account".to_owned()),
        }),
        source_id: "source-0001".to_owned(),
        capability_id: "capability-0001".to_owned(),
        provenance: BTreeSet::from(["account work".to_owned()]),
        source: ValidatedCredentialSource::Capability,
    };
    let stale = capability_for_binding(&binding, Some("generation-stale"));
    let current = capability_for_binding(&binding, Some("generation-current"));
    assert_ne!(stale, current);

    let temp = tempfile::tempdir().unwrap();
    let executor: Arc<dyn UsageProviderExecutor> = Arc::new(CountingExecutor {
        calls: AtomicUsize::new(0),
    });
    let client = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().to_owned()),
        executor,
    )
    .unwrap();
    client
        .reconcile_catalog(
            "generation-current".to_owned(),
            vec![UsageCatalogEntry {
                capability: current.clone(),
                revision: "credential-current".to_owned(),
            }],
        )
        .unwrap();

    assert_eq!(client.current(current).unwrap().generation, 0);
    assert_eq!(
        client.current(stale).unwrap_err().kind,
        UsageCoordinationErrorKind::CatalogRevoked
    );
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
fn concurrent_catalog_rotations_publish_one_complete_revision() {
    let temp = tempfile::tempdir().unwrap();
    let client = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().to_owned()),
        Arc::new(CountingExecutor {
            calls: AtomicUsize::new(0),
        }),
    )
    .unwrap();
    let account_a = capability();
    let account_b = second_capability();
    let entry_a = UsageCatalogEntry {
        capability: account_a.clone(),
        revision: "entry-a".to_owned(),
    };
    let entry_b = UsageCatalogEntry {
        capability: account_b.clone(),
        revision: "entry-b".to_owned(),
    };
    let lease = client.current_projection().unwrap().projection_id;
    let barrier = Arc::new(Barrier::new(3));
    let first = {
        let client = client.clone();
        let lease = lease.clone();
        let barrier = Arc::clone(&barrier);
        thread::spawn(move || {
            barrier.wait();
            client.reconcile_catalog_if_projection(
                Some(lease),
                "catalog-a".to_owned(),
                vec![entry_a],
            )
        })
    };
    let second = {
        let client = client.clone();
        let lease = lease.clone();
        let barrier = Arc::clone(&barrier);
        thread::spawn(move || {
            barrier.wait();
            client.reconcile_catalog_if_projection(
                Some(lease),
                "catalog-b".to_owned(),
                vec![entry_b],
            )
        })
    };
    barrier.wait();
    let first = first.join().unwrap();
    let second = second.join().unwrap();
    let (winner, rejected) = match (first, second) {
        (Ok(winner), Err(rejected)) | (Err(rejected), Ok(winner)) => (winner, rejected),
        (Ok(_), Ok(_)) => panic!("two catalog rotations committed"),
        (Err(first), Err(second)) => {
            panic!("both catalog rotations rejected: {first:?}; {second:?}")
        }
    };
    assert_eq!(
        rejected.kind,
        UsageCoordinationErrorKind::CatalogRevisionConflict
    );

    let final_projection = client.current_projection().unwrap();
    assert_eq!(
        final_projection.discovery_revision,
        winner.discovery_revision
    );
    match winner.discovery_revision.as_str() {
        "catalog-a" => {
            assert_eq!(client.current(account_a).unwrap().generation, 0);
            assert_eq!(
                client.current(account_b).unwrap_err().kind,
                UsageCoordinationErrorKind::CatalogRevoked
            );
        }
        "catalog-b" => {
            assert_eq!(client.current(account_b).unwrap().generation, 0);
            assert_eq!(
                client.current(account_a).unwrap_err().kind,
                UsageCoordinationErrorKind::CatalogRevoked
            );
        }
        revision => panic!("mixed or unknown catalog revision: {revision}"),
    }
}

#[test]
fn catalog_cas_rejects_a_stale_rotation_after_a_newer_winner() {
    let temp = tempfile::tempdir().unwrap();
    let client = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().to_owned()),
        Arc::new(CountingExecutor {
            calls: AtomicUsize::new(0),
        }),
    )
    .unwrap();
    let lease = client.current_projection().unwrap().projection_id;
    let winning = UsageCatalogEntry {
        capability: capability(),
        revision: "entry-winning".to_owned(),
    };
    let stale = UsageCatalogEntry {
        capability: second_capability(),
        revision: "entry-stale".to_owned(),
    };

    let winner = client
        .reconcile_catalog_if_projection(
            Some(lease.clone()),
            "catalog-winning".to_owned(),
            vec![winning],
        )
        .unwrap();
    let error = client
        .reconcile_catalog_if_projection(Some(lease), "catalog-stale".to_owned(), vec![stale])
        .unwrap_err();

    assert_eq!(
        error.kind,
        UsageCoordinationErrorKind::CatalogRevisionConflict
    );
    assert_eq!(client.current_projection().unwrap(), winner);
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
fn existing_broker_reconcile_revokes_without_returning_stale_projection() {
    let temp = tempfile::tempdir().unwrap();
    let executor: Arc<dyn UsageProviderExecutor> = Arc::new(CountingExecutor {
        calls: AtomicUsize::new(0),
    });
    let client = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().to_owned()),
        executor,
    )
    .unwrap();
    let entry = UsageCatalogEntry {
        capability: capability(),
        revision: "credential-a".to_owned(),
    };

    let admitted = client
        .reconcile_catalog("catalog-a".to_owned(), vec![entry.clone()])
        .unwrap();
    let queued = client.refresh(capability(), 0, true).unwrap();
    let completed = client
        .join(capability(), queued.generation, Duration::from_secs(2))
        .unwrap();
    assert_eq!(completed.phase, UsageRefreshPhase::Completed);

    let removed = client
        .reconcile_catalog("catalog-b".to_owned(), Vec::new())
        .unwrap();
    assert_eq!(removed.broker_instance_id, admitted.broker_instance_id);
    assert_eq!(removed.discovery_revision, "catalog-b");
    assert_eq!(
        removed.providers[0].accounts[0].canonical_account_id,
        capability().account_id
    );
    assert_eq!(
        removed.providers[0].accounts[0].status_label.as_deref(),
        Some("removed")
    );
    assert_eq!(client.current_projection().unwrap(), removed);
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

    let error = client
        .current_for_capability(UsageAccountCapability {
            account_id: "account-a".to_owned(),
            surface_id: "claude".to_owned(),
        })
        .unwrap_err();
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

    // An explicit operator refresh bypasses the success cooldown exactly once.
    let forced = client.refresh_due(true);
    assert!(forced.iter().all(|(_, result)| result.is_ok()));
    for (_, result) in &forced {
        let view = result.as_ref().unwrap();
        assert_eq!(view.generation, 2);
        client
            .join(
                view.capability.clone(),
                view.generation,
                Duration::from_secs(5),
            )
            .unwrap();
    }
    assert_eq!(executor.calls.load(Ordering::SeqCst), 4);
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

struct StallOneExecutor {
    slow: UsageAccountCapability,
    release: Mutex<mpsc::Receiver<()>>,
}

impl UsageProviderExecutor for StallOneExecutor {
    fn probe(&self, capability: &UsageAccountCapability, _generation: u64) -> ProviderProbeOutcome {
        if *capability == self.slow {
            self.release
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(15))
                .unwrap();
        }
        ProviderProbeOutcome::success(quota_view())
    }
}

#[test]
fn healthy_accounts_publish_while_one_account_stalls() {
    let temp = tempfile::tempdir().unwrap();
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let executor: Arc<dyn UsageProviderExecutor> = Arc::new(StallOneExecutor {
        slow: second_capability(),
        release: Mutex::new(release_rx),
    });
    let client = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().to_owned()),
        executor,
    )
    .unwrap();

    let before = client.current_projection().unwrap();
    let opened = client.subscribe_all([capability(), second_capability()]);
    assert!(opened.iter().all(|(_, result)| result.is_ok()));
    let fast = opened
        .iter()
        .find(|(item, _)| *item == capability())
        .unwrap()
        .1
        .as_ref()
        .unwrap()
        .clone();
    client
        .join(capability(), fast.generation, Duration::from_secs(5))
        .unwrap();

    // The healthy account is published with data while the stalled account
    // keeps its refreshing state; the catalog revision never changes.
    let partial = client.current_projection().unwrap();
    partial.validate().unwrap();
    assert_eq!(partial.discovery_revision, before.discovery_revision);
    assert!(partial.broker_generation > before.broker_generation);
    assert_eq!(
        partial.refresh_state,
        UsageProjectionRefreshStateV1::Refreshing
    );
    let providers = partial
        .providers
        .iter()
        .map(|provider| provider.provider_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(providers, vec!["claude", "codex"]);
    let fast_account = partial.providers[0]
        .accounts
        .iter()
        .find(|account| account.canonical_account_id == "abc123")
        .unwrap();
    assert_eq!(fast_account.freshness.phase, UsageFreshnessPhaseV1::Current);
    assert!(!fast_account.windows.is_empty());
    let slow_account = partial.providers[1]
        .accounts
        .iter()
        .find(|account| account.canonical_account_id == "def456")
        .unwrap();
    assert_eq!(
        slow_account.freshness.phase,
        UsageFreshnessPhaseV1::Refreshing
    );
    assert!(slow_account.windows.is_empty());

    release_tx.send(()).unwrap();
    let slow = opened
        .iter()
        .find(|(item, _)| *item == second_capability())
        .unwrap()
        .1
        .as_ref()
        .unwrap()
        .clone();
    client
        .join(second_capability(), slow.generation, Duration::from_secs(5))
        .unwrap();
    let settled = client.current_projection().unwrap();
    settled.validate().unwrap();
    assert_eq!(settled.discovery_revision, before.discovery_revision);
    assert!(settled.broker_generation > partial.broker_generation);
    assert_eq!(settled.refresh_state, UsageProjectionRefreshStateV1::Idle);
    assert!(
        settled
            .providers
            .iter()
            .flat_map(|provider| &provider.accounts)
            .all(|account| account.freshness.phase == UsageFreshnessPhaseV1::Current)
    );
}

#[test]
fn projection_refresh_runs_due_checks_and_join_settles() {
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
    client.subscribe(capability()).unwrap();
    client
        .join(capability(), 1, Duration::from_secs(5))
        .unwrap();
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);

    // A non-forced projection refresh reuses the still-fresh observation.
    let reused = client.request_refresh(None, false).unwrap();
    reused.validate().unwrap();
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    assert!(
        reused
            .providers
            .iter()
            .flat_map(|provider| &provider.accounts)
            .any(|account| account.canonical_account_id == "abc123")
    );

    // A forced projection refresh starts one new generation and the join
    // observes it settle without cancelling broker ownership.
    //
    // Join returns a superseding publication immediately by design, and
    // every intermediate publish mints a fresh publication id, so a single
    // join can observe a still-Refreshing snapshot under load. Chase the
    // chain until Idle or the deadline, like any correct caller must.
    let refreshing = client.request_refresh(None, true).unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut target = refreshing.projection_id.clone();
    let settled = loop {
        let observed = client
            .join_publication(target.clone(), Duration::from_secs(5))
            .unwrap();
        if observed.refresh_state == UsageProjectionRefreshStateV1::Idle
            || Instant::now() >= deadline
        {
            break observed;
        }
        target = observed.projection_id.clone();
    };
    assert_eq!(settled.refresh_state, UsageProjectionRefreshStateV1::Idle);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 2);

    // A superseded or unknown publication id returns the latest publication.
    let latest = client
        .join_publication("usage-broker:unknown".to_owned(), Duration::from_secs(5))
        .unwrap();
    assert_eq!(latest.projection_id, settled.projection_id);
}

#[test]
fn join_publication_timeout_leaves_broker_ownership_intact() {
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
    client.subscribe(capability()).unwrap();
    started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    // Wait for a quiesced refreshing publication: once the id is stable
    // across a ticker interval, no publish can interleave with the join below
    // until the probe is released.
    let refreshing = loop {
        let first = client.current_projection().unwrap();
        thread::park_timeout(Duration::from_millis(250));
        let second = client.current_projection().unwrap();
        if first.projection_id == second.projection_id
            && second.refresh_state == UsageProjectionRefreshStateV1::Refreshing
        {
            break second;
        }
    };

    let error = client
        .join_publication(refreshing.projection_id.clone(), Duration::from_millis(50))
        .unwrap_err();
    assert_eq!(error.kind, UsageCoordinationErrorKind::WaitTimeout);

    // The timed-out join cancelled nothing: releasing the probe still settles
    // the same account generation into a newer publication.
    release_tx.send(()).unwrap();
    let settled = client
        .join_publication(refreshing.projection_id, Duration::from_secs(5))
        .unwrap();
    assert_eq!(settled.refresh_state, UsageProjectionRefreshStateV1::Idle);
    assert!(settled.broker_generation > refreshing.broker_generation);
}

#[test]
fn probe_budget_returns_fast_and_expires_without_waiting() {
    let fast = probe::run_probe_with_budget(Duration::from_secs(5), || 7_u32).unwrap();
    assert_eq!(fast, 7);

    let started = Instant::now();
    let expired = probe::run_probe_with_budget(Duration::from_millis(20), || {
        thread::park_timeout(Duration::from_secs(30));
        7_u32
    });
    assert_eq!(expired, Err(probe::ProbeBudgetExpired));
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "budget expiry waited for the probe"
    );

    let timeout = probe::probe_timeout_outcome();
    let ProviderProbeOutcome::Failure {
        kind,
        message,
        retry_at_epoch,
    } = timeout
    else {
        panic!("budget expiry must report failure, never empty success");
    };
    assert_eq!(kind, UsageCoordinationErrorKind::ProviderTimeout);
    assert!(!message.is_empty());
    assert_eq!(retry_at_epoch, None);
}

#[test]
fn probe_budget_propagates_worker_panic_to_coordinator_classification() {
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        probe::run_probe_with_budget(Duration::from_secs(5), || {
            panic!("adapter panic must reach the coordinator")
        })
    }));
    assert!(
        outcome.is_err(),
        "worker panic must propagate to the caller"
    );
}

struct NoEnvResolver;

impl ProviderCredentialEnvResolver for NoEnvResolver {
    fn resolve_provider_credentials(
        &self,
        _config: &jackin_config::AppConfig,
        _workspace: Option<&jackin_core::WorkspaceName>,
        _role: Option<&str>,
        _keys: &[jackin_core::UsageCredentialEnvName],
    ) -> Vec<crate::host::ProviderCredentialEnvResolution> {
        Vec::new()
    }
}

#[test]
fn ensure_usage_broker_publishes_post_activation_discovery_not_stale_caller_input() {
    use crate::host::HostSurfaceId;

    let data_dir = tempfile::tempdir().unwrap();
    let config_root = tempfile::tempdir().unwrap();
    let operator_home = tempfile::tempdir().unwrap();
    let scope = UsageDiscoveryScope::HostDesktop {
        config_root: config_root.path().to_owned(),
        operator_home: operator_home.path().to_owned(),
    };
    let resolver: Arc<dyn ProviderCredentialEnvResolver> = Arc::new(NoEnvResolver);
    // Broker already serving (as after any prior activation).
    let _running = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(data_dir.path().to_owned()),
        Arc::new(CountingExecutor {
            calls: AtomicUsize::new(0),
        }),
    )
    .unwrap();

    // Stale caller generation: one admitted account at a caller-side
    // revision, simulating staged desktop discovery that predates the
    // current tree (the tree here is empty).
    let stale = ValidatedUsageDiscovery {
        config_generation: Some("stale-caller-rev".to_owned()),
        accounts: Vec::new(),
        diagnostics: Vec::new(),
        candidates: Vec::new(),
        bindings: vec![ValidatedCredentialBinding {
            surface: HostSurfaceId::Amp,
            identity: None,
            source_id: "stale-source".to_owned(),
            capability_id: "stale-capability".to_owned(),
            provenance: BTreeSet::from(["account stale-test".to_owned()]),
            source: ValidatedCredentialSource::Capability,
        }],
    };
    let expected_capability =
        capability_for_binding(&stale.bindings[0], stale.config_generation.as_deref());
    let handle = ensure_usage_broker(
        UsageBrokerConfig::for_data_dir(data_dir.path().to_owned()),
        scope.clone(),
        stale,
        Arc::clone(&resolver),
    )
    .unwrap();

    // The allowlist still derives from the caller's admitted set ...
    assert_eq!(handle.capabilities, vec![expected_capability]);
    // ... but the published catalog derives from post-activation discovery
    // (the empty tree here), never the stale caller revision.
    let fresh = validate_usage_sources(
        discover_usage_sources(&scope, resolver.as_ref()).unwrap(),
        resolver.as_ref(),
    );
    let expected_revision = fresh
        .config_generation
        .clone()
        .unwrap_or_else(|| "empty".to_owned());
    assert_ne!(expected_revision, "stale-caller-rev");
    let projection = handle.client.current_projection().unwrap();
    assert_eq!(projection.discovery_revision, expected_revision);
    assert_eq!(handle.catalog_lease, projection.projection_id);
}

#[test]
fn sequential_reconcile_after_fresh_read_still_accepts_last_writer() {
    // Documents the broker-level contract the activation ordering above
    // defends: the projection fence rejects CONCURRENT stale writers (see
    // `catalog_cas_rejects_a_stale_rotation_after_a_newer_winner`), but a
    // stale writer that reads AFTER the fresh publication still passes the
    // fence. That is why `ensure_usage_broker` must publish post-activation
    // discovery rather than trusting caller input of any age.
    let temp = tempfile::tempdir().unwrap();
    let client = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().to_owned()),
        Arc::new(CountingExecutor {
            calls: AtomicUsize::new(0),
        }),
    )
    .unwrap();
    let fresh = UsageCatalogEntry {
        capability: capability(),
        revision: "entry-fresh".to_owned(),
    };
    let stale = UsageCatalogEntry {
        capability: second_capability(),
        revision: "entry-stale".to_owned(),
    };

    let first = client.current_projection().unwrap().projection_id;
    let winner = client
        .reconcile_catalog_if_projection(Some(first), "catalog-fresh".to_owned(), vec![fresh])
        .unwrap();
    // Stale writer reads the fresh publication, then overwrites with older
    // data: the fence passes because the read was current.
    let read_after_fresh = client.current_projection().unwrap().projection_id;
    assert_eq!(read_after_fresh, winner.projection_id);
    let overwritten = client
        .reconcile_catalog_if_projection(
            Some(read_after_fresh),
            "catalog-stale".to_owned(),
            vec![stale],
        )
        .unwrap();
    assert_eq!(overwritten.discovery_revision, "catalog-stale");
}
