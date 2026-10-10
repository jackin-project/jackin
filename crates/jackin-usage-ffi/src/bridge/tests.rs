// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

use jackin_protocol::usage_broker::{
    UsageAccountV1, UsageCalendarPeriodV1, UsageFreshnessPhaseV1, UsageFreshnessV1,
    UsageIdentityKindV1, UsageLifecycleV1, UsageLimitWindowV1, UsageMembershipStateV1,
    UsageMetricGroupKindV1, UsageMetricGroupV1, UsageMetricPeriodV1, UsageMetricScopeV1,
    UsageMetricValueV1, UsagePercent, UsageProjectionRefreshStateV1, UsageProjectionSchemaV1,
    UsageProviderV1, UsageQuotaStateV1, UsageWindowCategoryV1,
};

fn open_bridge(dir: &std::path::Path) -> UsageMenuBarBridge {
    let bridge = UsageMenuBarBridge::create();
    bridge
        .open_runtime(OpenConfig {
            data_dir_override: Some(dir.display().to_string()),
            config_root_override: Some(dir.join("config").display().to_string()),
            refresh_floor_secs: 120,
            enabled_surface_ids: vec!["codex".to_owned(), "claude".to_owned()],
            allow_live_probes: false,
        })
        .expect("open offline bridge");
    bridge
}

#[test]
fn broker_conflict_remains_a_typed_bridge_failure() {
    let error = map_coordination_err(jackin_protocol::usage_broker::UsageCoordinationError {
        kind: jackin_protocol::usage_broker::UsageCoordinationErrorKind::BrokerConflict,
        message: "another service owns the usage broker lease".to_owned(),
    });

    assert_eq!(
        error,
        UsageBridgeError::rejected(
            "coordination_broker_conflict",
            "another service owns the usage broker lease"
        )
    );
}

fn fixture_projection() -> UsageProjectionV1 {
    let fresh = UsageFreshnessV1 {
        generation: 1,
        phase: UsageFreshnessPhaseV1::Current,
        last_good_at_epoch: Some(1_800_000_000),
        retry_at_epoch: None,
        is_stale: false,
    };
    let group = UsageMetricGroupV1 {
        group_id: "weekly-group".to_owned(),
        rank: 0,
        kind: UsageMetricGroupKindV1::Window,
        label: "Weekly quota".to_owned(),
        scope: UsageMetricScopeV1 {
            model: Some("gpt-5".to_owned()),
            ..UsageMetricScopeV1::default()
        },
        observed_at_epoch: Some(1_800_000_000),
        fetched_at_epoch: 1_800_000_001,
        last_success_at_epoch: Some(1_800_000_000),
        phase: UsageFreshnessPhaseV1::Current,
        is_stale: false,
        quota_state: UsageQuotaStateV1::Available,
        value: UsageMetricValueV1::Window {
            remaining_percent: Some(UsagePercent::new(64).expect("percent")),
            remaining_raw_percent: Some(64),
            used_percent: None,
            used_raw_percent: None,
            period: UsageMetricPeriodV1::Calendar {
                granularity: UsageCalendarPeriodV1::Weekly,
            },
            unit: Some("tokens".to_owned()),
        },
        reset_at_epoch: Some(1_800_100_000),
        renews_at_epoch: None,
        issues: Vec::new(),
    };
    let account = |id: &str, rank: u32, label: &str| UsageAccountV1 {
        canonical_account_id: id.to_owned(),
        identity_kind: UsageIdentityKindV1::ProviderAccountId,
        rank,
        display_label: label.to_owned(),
        plan_label: Some("Pro".to_owned()),
        status_label: None,
        lifecycle: UsageLifecycleV1::Available,
        freshness: fresh.clone(),
        provenance_count: 1,
        windows: vec![UsageLimitWindowV1 {
            window_id: format!("{id}-weekly"),
            rank: 0,
            category: UsageWindowCategoryV1::LongRange,
            label: "Weekly".to_owned(),
            value_label: "64% left".to_owned(),
            reset_label: "Resets in 3d".to_owned(),
            remaining_percent: Some(UsagePercent::new(64).expect("percent")),
            remaining_raw_percent: Some(64),
            used_percent: None,
            used_raw_percent: None,
            reset_at_epoch: Some(1_800_100_000),
            quota_state: UsageQuotaStateV1::Available,
            pace_label: None,
            runs_out_label: None,
        }],
        metric_groups: if rank == 0 {
            vec![group.clone()]
        } else {
            Vec::new()
        },
        credential_expires_at_epoch: None,
        issues: Vec::new(),
    };
    UsageProjectionV1 {
        schema_version: UsageProjectionSchemaV1,
        projection_id: "fixture-projection-1".to_owned(),
        generated_at_epoch: 1_800_000_001,
        discovery_revision: "fixture-catalog-1".to_owned(),
        broker_instance_id: "fixture-broker".to_owned(),
        broker_generation: 1,
        refresh_state: UsageProjectionRefreshStateV1::Idle,
        providers: vec![UsageProviderV1 {
            provider_id: "openai".to_owned(),
            display_name: "OpenAI".to_owned(),
            rank: 0,
            membership_state: UsageMembershipStateV1::Current,
            freshness: fresh.clone(),
            accounts: vec![
                account("canonical-openai-a", 0, "alice@example.test"),
                account("canonical-openai-b", 1, "bob@example.test"),
            ],
            issues: Vec::new(),
        }],
        unresolved: Vec::new(),
        issues: Vec::new(),
    }
}

#[test]
fn passive_bridge_reads_only_cached_broker_projection() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bridge = open_bridge(dir.path());
    bridge
        .apply_publication(fixture_projection())
        .expect("fixture publication");

    // Force is ignored in no-live-probe mode. This only attempts an attach-only
    // current-projection read and cannot start a provider executor.
    bridge
        .refresh(Some("codex".to_owned()), true)
        .expect("passive refresh");
    let snapshot = bridge.snapshot("codex".to_owned()).expect("snapshot");
    assert_eq!(snapshot.account_label, "alice@example.test");
    assert_eq!(snapshot.status, "fresh");
    assert_eq!(snapshot.buckets[0].remaining_percent, Some(64));
    assert_eq!(bridge.refresh_floor_secs().expect("floor"), 120);
    bridge.shutdown().expect("shutdown");
}

#[test]
fn passive_ffi_attach_does_not_dispatch_the_fake_broker_executor() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use jackin_protocol::usage_broker::{UsageAccountCapability, UsageCoordinationErrorKind};
    use jackin_usage::coordinator::{ProviderProbeOutcome, UsageProviderExecutor};
    use jackin_usage::host::{UsageBrokerConfig, ensure_usage_broker_with_executor};

    struct CountingExecutor(AtomicUsize);

    impl UsageProviderExecutor for CountingExecutor {
        fn probe(
            &self,
            _capability: &UsageAccountCapability,
            _generation: u64,
        ) -> ProviderProbeOutcome {
            self.0.fetch_add(1, Ordering::SeqCst);
            ProviderProbeOutcome::Failure {
                kind: UsageCoordinationErrorKind::Unavailable,
                message: "unexpected probe in passive FFI fixture".to_owned(),
                retry_at_epoch: None,
            }
        }
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let executor = Arc::new(CountingExecutor(AtomicUsize::new(0)));
    let executor_trait: Arc<dyn UsageProviderExecutor> = Arc::<CountingExecutor>::clone(&executor);
    let client = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(dir.path().to_owned()),
        executor_trait,
    )
    .expect("start fake broker");
    let before = client.current_projection().expect("initial publication");

    let bridge = open_bridge(dir.path());
    bridge.refresh(None, false).expect("nonforced refresh");
    bridge
        .refresh(None, true)
        .expect("force refresh stays disabled");
    let after = client.current_projection().expect("latest publication");
    assert_eq!(before.projection_id, after.projection_id);
    assert_eq!(executor.0.load(Ordering::SeqCst), 0);
    bridge.shutdown().expect("shutdown");
}

#[test]
fn passive_bridge_does_not_parse_host_provider_configuration() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config_root = dir.path().join("config");
    std::fs::create_dir_all(&config_root).expect("config root");
    std::fs::write(
        config_root.join("config.toml"),
        "this is not valid host provider configuration",
    )
    .expect("invalid config fixture");

    let bridge = open_bridge(dir.path());
    assert!(!bridge.list_surfaces().expect("surfaces").is_empty());
    bridge.shutdown().expect("shutdown");
}

#[test]
fn native_selection_and_details_keep_broker_canonical_identity_and_types() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bridge = open_bridge(dir.path());
    bridge
        .apply_publication(fixture_projection())
        .expect("fixture publication");

    let accounts = bridge
        .list_accounts(Some("codex".to_owned()))
        .expect("accounts");
    assert_eq!(accounts.len(), 2);
    assert_eq!(accounts[0].account_key, "canonical-openai-a");
    assert!(accounts[0].selected);
    let snapshot = bridge.snapshot("codex".to_owned()).expect("typed snapshot");
    assert!(
        snapshot
            .detail_presentation
            .rows
            .iter()
            .any(|row| row.row_id == "weekly-group" && row.display_label.contains("weekly"))
    );

    bridge
        .set_selected_account("codex".to_owned(), "canonical-openai-b".to_owned())
        .expect("select canonical account");
    let projection = bridge.desktop_projection(3).expect("desktop projection");
    let codex = projection
        .providers
        .iter()
        .find(|provider| provider.group.surface_id == "codex")
        .expect("codex provider");
    assert_eq!(codex.selected_account_route.status, "available");
    assert_eq!(
        codex.selected_account_route.account_key.as_deref(),
        Some("canonical-openai-b")
    );
    assert_eq!(codex.selected_usage.account_label, "bob@example.test");

    bridge
        .set_selected_account("codex".to_owned(), "unknown-account".to_owned())
        .expect_err("reject nonmember canonical id");
    bridge.shutdown().expect("shutdown");
}

#[test]
fn missing_selected_account_never_falls_back_to_a_published_sibling() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bridge = open_bridge(dir.path());
    bridge
        .apply_publication(fixture_projection())
        .expect("fixture publication");
    bridge
        .set_selected_account("codex".to_owned(), "canonical-openai-b".to_owned())
        .expect("select canonical account");

    let mut publication = fixture_projection();
    publication.projection_id = "fixture-projection-2".to_owned();
    publication.broker_generation = 2;
    publication.providers[0].accounts.pop();
    bridge
        .apply_publication(publication)
        .expect("account removal publication");

    let projection = bridge.desktop_projection(3).expect("projection");
    let codex = projection
        .providers
        .iter()
        .find(|provider| provider.group.surface_id == "codex")
        .expect("Codex provider");
    assert_eq!(codex.group.accounts.len(), 1);
    assert!(!codex.group.accounts[0].selected);
    assert_eq!(codex.selected_account_route.status, "unavailable");
    assert_eq!(
        codex.selected_account_route.account_key.as_deref(),
        Some("canonical-openai-b")
    );
    assert_eq!(codex.selected_usage.status, "unavailable");
    assert_ne!(codex.selected_usage.account_label, "alice@example.test");
    assert_ne!(
        codex.selected_usage.identity.account_label,
        "alice@example.test"
    );
    assert!(codex.selected_usage.buckets.is_empty());
    bridge.shutdown().expect("shutdown");
}

#[test]
fn projection_glance_preserves_percent_and_reset_preferences() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bridge = open_bridge(dir.path());
    bridge
        .apply_publication(fixture_projection())
        .expect("fixture publication");
    bridge
        .set_format_prefs(UsageFormatPrefsDto {
            percent_style: "used".to_owned(),
            reset_style: "exact_clock".to_owned(),
        })
        .expect("format preferences");

    let codex = bridge
        .provider_glance_rows()
        .expect("glance rows")
        .into_iter()
        .find(|row| row.surface_id == "codex")
        .expect("codex glance row");
    assert_eq!(codex.bar_label, "36%");
    assert_eq!(codex.headline, "gpt-5 36% used");
    assert!(codex.reset_label.is_some());
    assert!(codex.exact_reset.is_some());

    assert_eq!(
        bridge
            .compact_status_bar_label_for("codex".to_owned())
            .expect("compact label")
            .as_deref(),
        Some("Cx 36%")
    );
    bridge.shutdown().expect("shutdown");
}

#[test]
fn nonforced_refresh_reads_cache_and_resets_only_the_local_floor() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bridge = open_bridge(dir.path());
    bridge
        .apply_publication(fixture_projection())
        .expect("fixture publication");
    let before = bridge
        .with_state(|state| Ok(state.runtime.projection().projection_id.clone()))
        .expect("before id");

    bridge.refresh(None, false).expect("nonforced cached read");
    let after = bridge
        .with_state(|state| Ok(state.runtime.projection().projection_id.clone()))
        .expect("after id");
    assert_eq!(before, after);
    assert!(!bridge.refresh_due().expect("refresh due"));
    bridge.shutdown().expect("shutdown");
}

#[test]
fn live_ffi_explicit_refresh_does_not_probe_in_the_client_process() {
    use jackin_protocol::usage_broker::{UsageAccountCapability, UsageCoordinationErrorKind};
    use jackin_usage::coordinator::{ProviderProbeOutcome, UsageProviderExecutor};
    use jackin_usage::host::{UsageBrokerConfig, ensure_usage_broker_with_executor};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingExecutor(AtomicUsize);

    impl UsageProviderExecutor for CountingExecutor {
        fn probe(
            &self,
            _capability: &UsageAccountCapability,
            _generation: u64,
        ) -> ProviderProbeOutcome {
            self.0.fetch_add(1, Ordering::SeqCst);
            ProviderProbeOutcome::Failure {
                kind: UsageCoordinationErrorKind::ProviderUnavailable,
                message: "fake broker fixture has no provider adapter".to_owned(),
                retry_at_epoch: None,
            }
        }
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let executor = Arc::new(CountingExecutor(AtomicUsize::new(0)));
    let executor_trait: Arc<dyn UsageProviderExecutor> = Arc::<CountingExecutor>::clone(&executor);
    let _client = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(dir.path().to_owned()),
        executor_trait,
    )
    .expect("start fake broker");
    let bridge = UsageMenuBarBridge::create();
    bridge
        .open_runtime(OpenConfig {
            data_dir_override: Some(dir.path().display().to_string()),
            config_root_override: Some(dir.path().join("config").display().to_string()),
            refresh_floor_secs: 120,
            enabled_surface_ids: vec!["codex".to_owned()],
            allow_live_probes: true,
        })
        .expect("attach live bridge to existing fake broker");
    bridge
        .refresh(Some("codex".to_owned()), false)
        .expect("periodic cache read");
    assert_eq!(executor.0.load(Ordering::SeqCst), 0);

    bridge
        .refresh(Some("codex".to_owned()), true)
        .expect("explicit operator refresh");
    // This fake broker has no broker-owned catalog refresher. Explicit
    // refresh intent stays inside the broker process; the FFI client never
    // falls back to a provider call.
    assert_eq!(executor.0.load(Ordering::SeqCst), 0);
    bridge.shutdown().expect("shutdown");
}
