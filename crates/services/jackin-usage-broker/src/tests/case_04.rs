// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn broker_catalog_admits_current_identity_and_rejects_stale_identity() {
    use jackin_usage_host_accounts::{CanonicalAccountIdentity, CanonicalAccountSubject};
    use jackin_usage_host_presentation::HostSurfaceId;

    let binding = ValidatedCredentialBinding {
        surface: HostSurfaceId::Claude,
        identity: Some(CanonicalAccountIdentity {
            surface: HostSurfaceId::Claude,
            subject: CanonicalAccountSubject::ProviderId("provider-account".to_owned()),
        }),
        source_id: "source-0001".to_owned(),
        capability_id: "capability-0001".to_owned(),
        credential_revision: "credential-revision".to_owned(),
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
fn broker_catalog_match_requires_full_revision_and_entry_revisions() {
    use jackin_usage_host_accounts::{CanonicalAccountIdentity, CanonicalAccountSubject};
    use jackin_usage_host_presentation::HostSurfaceId;

    let discovery = ValidatedUsageDiscovery {
        config_generation: Some("generation-current".to_owned()),
        accounts: Vec::new(),
        diagnostics: Vec::new(),
        candidates: Vec::new(),
        bindings: vec![ValidatedCredentialBinding {
            surface: HostSurfaceId::Claude,
            identity: Some(CanonicalAccountIdentity {
                surface: HostSurfaceId::Claude,
                subject: CanonicalAccountSubject::ProviderId("provider-account".to_owned()),
            }),
            source_id: "source-0001".to_owned(),
            capability_id: "capability-0001".to_owned(),
            credential_revision: "credential-revision-a".to_owned(),
            provenance: BTreeSet::from(["account work".to_owned()]),
            source: ValidatedCredentialSource::Capability,
        }],
    };
    let entries = usage_catalog_entries(&discovery);
    ensure_catalog_matches(&discovery, "generation-current", &entries).unwrap();

    let mut changed_entries = entries.clone();
    changed_entries[0].revision.push_str("-changed");
    assert_eq!(
        ensure_catalog_matches(&discovery, "generation-current", &changed_entries)
            .unwrap_err()
            .kind,
        UsageCoordinationErrorKind::CatalogRevisionConflict
    );
    assert_eq!(
        ensure_catalog_matches(&discovery, "generation-old", &entries)
            .unwrap_err()
            .kind,
        UsageCoordinationErrorKind::CatalogRevisionConflict
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
