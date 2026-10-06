// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn open_runtime_publishes_catalog_before_broker_admission() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_account_config(&dir.path().join("config"), "fixture", "anthropic");
    let broker = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(dir.path().to_owned()),
        Arc::new(BlockingBrokerExecutor::new()),
    )
    .expect("test broker");

    let bridge = open_bridge_with_live(dir.path(), true);
    let discovery = bridge
        .inner
        .lock()
        .unwrap()
        .validated_discovery()
        .expect("discovery");
    let admitted = usage_broker_capabilities(&discovery);
    assert_eq!(
        admitted.len(),
        1,
        "production activation must admit discovery"
    );
    assert_eq!(broker.current(admitted[0].clone()).unwrap().generation, 0);
    let synthetic = UsageAccountCapability {
        account_id: "not-in-desktop-discovery".to_owned(),
        surface_id: "claude".to_owned(),
    };
    let error = broker.current(synthetic).expect_err("catalog fence");
    assert_eq!(error.kind, UsageCoordinationErrorKind::CatalogRevoked);
}

#[test]
fn open_runtime_activation_failure_is_error_and_keeps_runtime_closed() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config_root = dir.path().join("config");
    write_account_config(&config_root, "fixture", "anthropic");
    let bridge = UsageMenuBarBridge::create();
    let host_config = to_host_config(OpenConfig {
        data_dir_override: Some(dir.path().display().to_string()),
        config_root_override: Some(config_root.display().to_string()),
        refresh_floor_secs: 120,
        enabled_surface_ids: vec!["claude".to_owned()],
        allow_live_probes: true,
    })
    .expect("host config");
    let mut broker_config = UsageBrokerConfig::for_data_dir(dir.path().to_owned());
    broker_config.service_executable = None;

    let error = bridge
        .open_runtime_with_config(host_config, broker_config)
        .expect_err("activation failure must cross the bridge");
    assert!(matches!(
        error,
        UsageBridgeError::Rejected { ref code, .. } if code == "coordination_unavailable"
    ));
    assert!(matches!(
        bridge.list_surfaces(),
        Err(UsageBridgeError::RuntimeUnavailable)
    ));
    assert!(bridge.broker.lock().unwrap().is_none());
}

#[test]
fn failed_production_rotation_retains_last_good_discovery_and_broker() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config_root = dir.path().join("config");
    write_account_config(&config_root, "fixture_a", "anthropic");
    let broker = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(dir.path().to_owned()),
        Arc::new(BlockingBrokerExecutor::new()),
    )
    .expect("test broker");
    let bridge = open_bridge_with_live(dir.path(), true);
    let before = bridge
        .inner
        .lock()
        .unwrap()
        .validated_discovery()
        .expect("initial discovery");
    let before_capabilities = usage_broker_capabilities(&before);
    assert_eq!(before_capabilities.len(), 1);

    write_account_config(&config_root, "fixture_b", "anthropic");
    {
        let mut current = bridge.broker.lock().unwrap();
        let broker = current.as_mut().expect("attached broker");
        broker.config.data_dir = dir.path().join("failed-rotation-broker");
        broker.config.service_executable = Some(dir.path().join("missing-broker"));
    }

    let error = bridge
        .refresh(None, true)
        .expect_err("failed rotation must be visible");
    assert!(matches!(
        error,
        UsageBridgeError::Rejected { ref code, .. } if code == "coordination_unavailable"
    ));
    let after = bridge
        .inner
        .lock()
        .unwrap()
        .validated_discovery()
        .expect("last-good discovery");
    assert_eq!(after.config_generation, before.config_generation);
    assert_eq!(usage_broker_capabilities(&after), before_capabilities);
    assert_eq!(
        broker
            .current(before_capabilities[0].clone())
            .unwrap()
            .generation,
        0
    );
}

#[test]
fn production_rotation_revokes_in_flight_join_and_clears_bridge_phase() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config_root = dir.path().join("config");
    write_account_config(&config_root, "fixture", "anthropic");
    let executor = Arc::new(BlockingBrokerExecutor::new());
    let executor_handle = Arc::clone(&executor);
    let broker_executor: Arc<dyn UsageProviderExecutor> = executor_handle;
    let broker = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(dir.path().to_owned()),
        broker_executor,
    )
    .expect("test broker");
    let bridge = open_bridge_with_live(dir.path(), true);
    let capability = usage_broker_capabilities(
        &bridge
            .inner
            .lock()
            .unwrap()
            .validated_discovery()
            .expect("discovery"),
    )
    .pop()
    .expect("capability");

    bridge.refresh(None, true).expect("start refresh");
    executor.wait_started();
    let in_flight = broker.current(capability.clone()).expect("in-flight state");
    assert!(in_flight.phase.is_active());

    write_isolated_config(&config_root);
    bridge.refresh(None, true).expect("remove capability");
    assert!(!bridge.refresh_in_progress().expect("phase cleared"));
    let revoked = broker
        .join(capability, in_flight.generation, Duration::from_secs(1))
        .expect_err("revoked join");
    assert_eq!(revoked.kind, UsageCoordinationErrorKind::CatalogRevoked);

    executor.release();
    let deadline = Instant::now() + Duration::from_secs(3);
    while bridge.refresh_in_progress().expect("phase poll") && Instant::now() < deadline {
        std::thread::park_timeout(Duration::from_millis(10));
    }
    assert!(!bridge.refresh_in_progress().expect("phase settled"));
}

#[test]
fn broker_client_refresh_returns_immediately_and_joins_one_generation() {
    let dir = tempfile::tempdir().unwrap();
    let bridge = UsageMenuBarBridge::create();
    let discovery_scope = UsageDiscoveryScope::Capsule {
        forwarded_accounts: vec![ForwardedUsageAccount {
            surface_id: "claude".to_owned(),
            capability_id: "fixture-capability".to_owned(),
            account_label: Some("broker@example.test".to_owned()),
        }],
    };
    let runtime_config = HostRuntimeConfig {
        data_dir: dir.path().join("runtime"),
        refresh_floor_secs: 60,
        enabled_surface_ids: vec!["claude".to_owned()],
        probe_policy: HostProbePolicy::Live,
        discovery_scope: discovery_scope.clone(),
    };
    {
        bridge
            .inner
            .lock()
            .unwrap()
            .open_with_discovery(runtime_config, bridge.credential_resolver.as_ref())
            .unwrap();
    }
    let discovery = bridge.inner.lock().unwrap().validated_discovery().unwrap();
    let capabilities = usage_broker_capabilities(&discovery);
    let executor = Arc::new(BlockingBrokerExecutor::new());
    let concrete = Arc::clone(&executor);
    let broker_executor: Arc<dyn UsageProviderExecutor> = concrete;
    let broker_config = UsageBrokerConfig::for_data_dir(dir.path().join("broker"));
    let client = ensure_usage_broker_with_executor(broker_config.clone(), broker_executor).unwrap();
    *bridge.broker.lock().unwrap() = Some(DesktopBroker {
        client,
        capabilities,
        catalog_lease: "test-lease".to_owned(),
        config: broker_config,
        scope: discovery_scope,
    });

    let started = Instant::now();
    bridge.refresh(None, true).unwrap();
    assert!(started.elapsed() < Duration::from_secs(1));
    executor.wait_started();
    assert!(bridge.refresh_in_progress().unwrap());
    bridge.refresh(None, true).unwrap();
    executor.release();

    let deadline = Instant::now() + Duration::from_secs(3);
    while bridge.refresh_in_progress().unwrap() && Instant::now() < deadline {
        std::thread::park_timeout(Duration::from_millis(10));
    }
    assert!(!bridge.refresh_in_progress().unwrap());
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    let snapshot = bridge.snapshot("claude".to_owned()).unwrap();
    assert_eq!(snapshot.account_label, "broker@example.test");
    assert_eq!(snapshot.buckets[0].remaining_percent, Some(64));
}

#[test]
fn disc_diagnostics_export_only_sanitized_scope_and_copy() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config = dir.path().join("config");
    let workspaces = config.join("workspaces");
    write_isolated_config(&config);
    std::fs::create_dir_all(&workspaces).expect("workspaces");
    std::fs::write(
        workspaces.join("broken.toml"),
        "{fixture-secret op://vault/item",
    )
    .expect("workspace");

    let bridge = open_bridge(dir.path());
    let diagnostics = bridge.discovery_diagnostics().expect("diagnostics");
    let debug = format!("{diagnostics:?}");

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.scope_label == "workspace broken" && diagnostic.issue == "config_invalid"
    }));
    for forbidden in [
        "fixture-secret",
        "op://",
        dir.path().to_string_lossy().as_ref(),
    ] {
        assert!(!debug.contains(forbidden), "leaked {forbidden}");
    }
}

#[test]
fn panic_probe_is_contained() {
    let bridge = UsageMenuBarBridge::create();
    let err = bridge.panic_probe().expect_err("panic");
    assert!(matches!(err, UsageBridgeError::ContainedPanic { .. }));
    // Still usable after containment.
    bridge.shutdown().expect("shutdown");
}

#[test]
fn fixture_snapshot_round_trip_via_bridge() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bridge = open_bridge(dir.path());
    // Inject through host runtime underneath for offline proof.
    {
        let mut guard = bridge.inner.lock().expect("lock");
        let view = FocusedUsageView {
            focused_agent: Some("codex".to_owned()),
            focused_provider: Some("Codex".to_owned()),
            account: FocusedAccountHeader {
                provider_label: "OpenAI / Codex".to_owned(),
                account_label: "codex@example.com".to_owned(),
                username: None,
                plan_label: Some("Pro 20x".to_owned()),
                credential_origin: None,
            },
            buckets: vec![
                QuotaBucketView {
                    label: "Session".to_owned(),
                    used_label: Some("63% used".to_owned()),
                    limit_label: Some("100%".to_owned()),
                    remaining_percent: Some(37),
                    reset_label: Some("Resets in 2h".to_owned()),
                    resets_at: Some(99),
                    status_slot: Some(StatusSlot::Session),
                    pace_label: None,
                    status: UsageSnapshotStatus::Fresh,
                    used_money: None,
                    limit_money: None,
                    severity: UsageSeverity::Normal,
                },
                QuotaBucketView {
                    label: "Amp Free".to_owned(),
                    used_label: None,
                    limit_label: None,
                    remaining_percent: Some(61),
                    reset_label: Some("Resets daily".to_owned()),
                    resets_at: None,
                    status_slot: Some(StatusSlot::Daily),
                    pace_label: None,
                    status: UsageSnapshotStatus::Fresh,
                    used_money: None,
                    limit_money: None,
                    severity: UsageSeverity::Normal,
                },
            ],
            status: UsageSnapshotStatus::Fresh,
            source: UsageSource::ProviderApi,
            confidence: UsageConfidence::Authoritative,
            fetched_at_epoch: 1,
            updated_label: "just now".to_owned(),
            status_bar_label: "Codex Session: 63% used · 37% left".to_owned(),
            tabs: Vec::new(),
            last_error: None,
        };
        guard.inject_snapshot("codex", view).expect("inject");
    }
    let dto = bridge.snapshot("codex".to_owned()).expect("snapshot");
    assert_eq!(dto.status_bar_label, "Codex Session: 63% used · 37% left");
    assert_eq!(dto.buckets.len(), 2);
    assert_eq!(dto.buckets[0].remaining_percent, Some(37));
    assert_eq!(dto.buckets[0].resets_at, Some(99));
    assert_eq!(dto.buckets[0].status_slot.as_deref(), Some("session"));
    assert_eq!(dto.buckets[1].status_slot.as_deref(), Some("daily"));
    // Rust-owned presentation fields ride on the bucket DTO.
    assert_eq!(dto.buckets[0].meter_percent, Some(37));
    assert!(
        dto.buckets[0]
            .display_segments
            .contains(&"37% left".to_owned())
    );
    assert!(dto.buckets[0].display_label.contains("37% left"));
    assert_eq!(dto.status, "fresh");
    assert_eq!(dto.estimate_caption, None);
    let merged = bridge.merged_status_bar_label().expect("merged");
    assert!(merged.contains("63%"));
    let surfaces = bridge.list_surfaces().expect("list");
    assert!(surfaces.iter().any(|s| s.id == "codex" && s.enabled));
    assert!(surfaces.iter().any(|s| s.id == "amp" && !s.enabled));
    bridge
        .set_enabled("codex".to_owned(), false)
        .expect("disable");
    bridge.snapshot("codex".to_owned()).unwrap_err();
    bridge.shutdown().expect("shutdown");
    assert!(matches!(
        bridge.list_surfaces().expect_err("closed"),
        UsageBridgeError::RuntimeUnavailable | UsageBridgeError::Rejected { .. }
    ));
}
