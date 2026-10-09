// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
fn publisher_rejects_stale_catalog_lease_without_overwriting_winner() {
    use jackin_protocol::usage_broker::{
        UsageProjectionRefreshStateV1, UsageProjectionSchemaV1, UsageProjectionV1,
    };
    use jackin_usage_coordinator::{
        FileAccountStateStore, FileProjectionStateStore, UsageCoordinator, UsageCoordinatorConfig,
    };

    let temp = tempfile::tempdir().unwrap();
    let account = capability();
    let coordinator = Arc::new(UsageCoordinator::new(
        Arc::new(CountingExecutor {
            calls: AtomicUsize::new(0),
        }),
        Arc::new(FileAccountStateStore::at(temp.path().join("accounts"))),
        UsageCoordinatorConfig::default(),
    ));
    let projection = Arc::new(Mutex::new(UsageProjectionV1 {
        schema_version: UsageProjectionSchemaV1,
        projection_id: "test:0".to_owned(),
        generated_at_epoch: 1_000,
        discovery_revision: "catalog-initial".to_owned(),
        broker_instance_id: "test".to_owned(),
        broker_generation: 0,
        refresh_state: UsageProjectionRefreshStateV1::Idle,
        providers: Vec::new(),
        unresolved: Vec::new(),
        issues: Vec::new(),
    }));
    let publisher = publish::ProjectionPublisher::new(
        coordinator,
        projection,
        FileProjectionStateStore::under_data_dir(temp.path()),
    );
    let entry = UsageCatalogEntry {
        capability: account,
        revision: "credential-revision".to_owned(),
    };
    let observed_lease = "test:0";
    let winner = publisher
        .reconcile_catalog_if_projection(
            Some(observed_lease),
            "catalog-winner".to_owned(),
            vec![entry.clone()],
            1_001,
        )
        .unwrap();
    let rejected = publisher
        .reconcile_catalog_if_projection(
            Some(observed_lease),
            "catalog-stale".to_owned(),
            vec![entry],
            1_002,
        )
        .unwrap_err();

    assert_eq!(
        rejected.kind,
        UsageCoordinationErrorKind::CatalogRevisionConflict
    );
    assert_eq!(publisher.current_projection().unwrap(), winner);
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

    let error = client
        .current_for_capability(UsageAccountCapability {
            account_id: "account-a".to_owned(),
            surface_id: "claude".to_owned(),
        })
        .unwrap_err();
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unauthorized);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
}
