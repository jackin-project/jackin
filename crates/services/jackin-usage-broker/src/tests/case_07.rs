// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn catalog_conflict_fails_closed_after_bounded_retries() {
    use jackin_usage_host_presentation::HostSurfaceId;

    let temp = tempfile::tempdir().unwrap();
    let config = UsageBrokerConfig::for_data_dir(temp.path().to_owned());
    let _client = counting_broker(temp.path());

    let fresh = scripted_discovery(
        Some("generation-fresh"),
        &[("fresh", HostSurfaceId::Claude)],
    );
    let scans = Arc::new(AtomicUsize::new(0));
    let reconciles = Arc::new(AtomicUsize::new(0));
    let mut discover = {
        let scans = Arc::clone(&scans);
        move || -> Result<ValidatedUsageDiscovery, UsageCoordinationError> {
            scans.fetch_add(1, Ordering::SeqCst);
            Ok(fresh.clone())
        }
    };
    let mut reconcile = {
        let reconciles = Arc::clone(&reconciles);
        move |_client: &UsageBrokerClient,
              _expected_projection_id: Option<String>,
              _catalog_revision: String,
              _entries: Vec<UsageCatalogEntry>|
              -> Result<UsageProjectionV1, UsageCoordinationError> {
            reconciles.fetch_add(1, Ordering::SeqCst);
            Err(UsageCoordinationError {
                kind: UsageCoordinationErrorKind::CatalogRevisionConflict,
                message: "scripted conflict".to_owned(),
            })
        }
    };
    let error = ensure_usage_broker_with_hooks(
        &config,
        &activation_scope(&temp),
        scripted_discovery(None, &[]),
        &mut discover,
        &mut reconcile,
    )
    .unwrap_err();

    assert_eq!(
        error.kind,
        UsageCoordinationErrorKind::CatalogRevisionConflict
    );
    assert_eq!(
        reconciles.load(Ordering::SeqCst),
        BROKER_ACTIVATION_ATTEMPTS as usize
    );
    assert_eq!(
        scans.load(Ordering::SeqCst),
        BROKER_ACTIVATION_ATTEMPTS as usize,
        "every attempt must run its own post-lease discovery scan"
    );
}

#[test]
fn transient_empty_scan_does_not_wipe_live_catalog() {
    use jackin_usage_host_presentation::HostSurfaceId;

    let temp = tempfile::tempdir().unwrap();
    let config = UsageBrokerConfig::for_data_dir(temp.path().to_owned());
    let client = counting_broker(temp.path());

    let good = scripted_discovery(Some("generation-good"), &[("good", HostSurfaceId::Claude)]);
    let good_capability =
        capability_for_binding(&good.bindings[0], good.config_generation.as_deref());
    client
        .reconcile_catalog("generation-good".to_owned(), usage_catalog_entries(&good))
        .unwrap();

    // First scan observes a transient empty catalog; the confirmation scan
    // re-observes the live catalog. Pops resolve in scan order.
    let scripted = Arc::new(Mutex::new(vec![
        good.clone(),
        scripted_discovery(None, &[]),
    ]));
    let mut discover = {
        let scripted = Arc::clone(&scripted);
        move || -> Result<ValidatedUsageDiscovery, UsageCoordinationError> {
            Ok(scripted.lock().unwrap().pop().unwrap())
        }
    };
    let published_sizes = Arc::new(Mutex::new(Vec::<usize>::new()));
    let mut reconcile = {
        let published_sizes = Arc::clone(&published_sizes);
        move |client: &UsageBrokerClient,
              expected_projection_id: Option<String>,
              catalog_revision: String,
              entries: Vec<UsageCatalogEntry>| {
            published_sizes.lock().unwrap().push(entries.len());
            client.reconcile_catalog_if_projection(
                expected_projection_id,
                catalog_revision,
                entries,
            )
        }
    };
    let handle = ensure_usage_broker_with_hooks(
        &config,
        &activation_scope(&temp),
        good.clone(),
        &mut discover,
        &mut reconcile,
    )
    .unwrap();

    let published_sizes = published_sizes.lock().unwrap();
    assert_eq!(
        published_sizes.as_slice(),
        &[1],
        "transient empty scan must yield to the confirmation scan, never publish: {published_sizes:?}"
    );
    let final_projection = client.current_projection().unwrap();
    assert_eq!(final_projection.discovery_revision, "generation-good");
    assert_eq!(handle.catalog_lease, final_projection.projection_id);
    client.current(good_capability).unwrap();
}

#[test]
fn confirmed_empty_scan_still_revokes_live_catalog() {
    use jackin_usage_host_presentation::HostSurfaceId;

    let temp = tempfile::tempdir().unwrap();
    let config = UsageBrokerConfig::for_data_dir(temp.path().to_owned());
    let client = counting_broker(temp.path());

    let good = scripted_discovery(Some("generation-good"), &[("good", HostSurfaceId::Claude)]);
    let good_capability =
        capability_for_binding(&good.bindings[0], good.config_generation.as_deref());
    client
        .reconcile_catalog("generation-good".to_owned(), usage_catalog_entries(&good))
        .unwrap();

    // Two consecutive empty scans confirm a genuine removal: the revocation
    // must still publish.
    let mut discover = || -> Result<ValidatedUsageDiscovery, UsageCoordinationError> {
        Ok(scripted_discovery(None, &[]))
    };
    let mut reconcile = |client: &UsageBrokerClient,
                         expected_projection_id: Option<String>,
                         catalog_revision: String,
                         entries: Vec<UsageCatalogEntry>| {
        client.reconcile_catalog_if_projection(expected_projection_id, catalog_revision, entries)
    };
    let handle = ensure_usage_broker_with_hooks(
        &config,
        &activation_scope(&temp),
        good,
        &mut discover,
        &mut reconcile,
    )
    .unwrap();

    let final_projection = client.current_projection().unwrap();
    assert_eq!(final_projection.discovery_revision, "empty");
    assert_eq!(handle.catalog_lease, final_projection.projection_id);
    assert!(handle.capabilities.is_empty());
    assert_eq!(
        client.current(good_capability).unwrap_err().kind,
        UsageCoordinationErrorKind::CatalogRevoked
    );
}

#[test]
fn ensure_usage_broker_publishes_fresh_discovery_not_stale_caller_input() {
    use jackin_usage_host_presentation::HostSurfaceId;

    let data_dir = tempfile::tempdir().unwrap();
    let config_root = tempfile::tempdir().unwrap();
    let operator_home = tempfile::tempdir().unwrap();
    let scope = UsageDiscoveryScope::HostDesktop {
        config_root: config_root.path().to_owned(),
        operator_home: operator_home.path().to_owned(),
    };
    let resolver: Arc<dyn ProviderCredentialEnvResolver> = Arc::new(NoEnvResolver);
    // Broker already serving (as after any prior activation).
    let _running = counting_broker(data_dir.path());

    // Stale caller generation: one admitted account at a caller-side
    // revision, simulating staged desktop discovery that predates the
    // current tree (the tree here is empty).
    let stale = scripted_discovery(Some("stale-caller-rev"), &[("stale", HostSurfaceId::Amp)]);
    let handle = ensure_usage_broker(
        UsageBrokerConfig::for_data_dir(data_dir.path().to_owned()),
        scope.clone(),
        stale,
        Arc::clone(&resolver),
    )
    .unwrap();

    // The published catalog derives from post-lease discovery (the empty
    // tree here), never the stale caller revision ...
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
    // ... and the returned handle matches the published generation, not the
    // caller's admitted set.
    assert_eq!(handle.capabilities, usage_broker_capabilities(&fresh));
}

#[test]
fn sequential_reconcile_after_fresh_read_still_accepts_last_writer() {
    // Documents the broker-level contract the activation ordering above
    // defends: the projection fence rejects CONCURRENT stale writers (see
    // `catalog_cas_rejects_a_stale_rotation_after_a_newer_winner`), but a
    // stale writer that reads AFTER the fresh publication still passes the
    // fence. That is why `ensure_usage_broker` must publish post-lease
    // discovery resolved under the activation lock rather than trusting
    // caller input of any age.
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
