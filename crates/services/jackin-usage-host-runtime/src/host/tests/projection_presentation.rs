// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use jackin_protocol::usage_broker::{
    UsageAccountV1, UsageCalendarPeriodV1, UsageFreshnessPhaseV1, UsageFreshnessV1,
    UsageIdentityKindV1, UsageLifecycleV1, UsageMembershipStateV1, UsageMetricGroupKindV1,
    UsageMetricGroupV1, UsageMetricPeriodV1, UsageMetricScopeV1, UsageMetricValueV1, UsagePercent,
    UsageProjectionRefreshStateV1, UsageProjectionSchemaV1, UsageProjectionV1, UsageProviderV1,
    UsageQuotaStateV1, UsageUnresolvedV1,
};

use crate::host::{
    HostUsageProjectionConfig, HostUsageProjectionRuntime, HostUsageProjectionSelectedAccount,
};

fn projection(
    instance: &str,
    generation: u64,
    providers: Vec<UsageProviderV1>,
) -> UsageProjectionV1 {
    UsageProjectionV1 {
        schema_version: UsageProjectionSchemaV1,
        projection_id: format!("{instance}:{generation}"),
        generated_at_epoch: i64::try_from(generation).unwrap_or(i64::MAX),
        discovery_revision: format!("discovery-{generation}"),
        broker_instance_id: instance.to_owned(),
        broker_generation: generation,
        refresh_state: UsageProjectionRefreshStateV1::Idle,
        providers,
        unresolved: Vec::new(),
        issues: Vec::new(),
    }
}

fn provider(provider_id: &str, rank: u32, accounts: Vec<UsageAccountV1>) -> UsageProviderV1 {
    UsageProviderV1 {
        provider_id: provider_id.to_owned(),
        display_name: match provider_id {
            "openai" => "OpenAI",
            "anthropic" => "Anthropic",
            "amp" => "Amp",
            _ => "Other",
        }
        .to_owned(),
        rank,
        membership_state: UsageMembershipStateV1::Current,
        freshness: freshness(),
        accounts,
        issues: Vec::new(),
    }
}

fn account(
    canonical_account_id: &str,
    rank: u32,
    metric_groups: Vec<UsageMetricGroupV1>,
) -> UsageAccountV1 {
    UsageAccountV1 {
        canonical_account_id: canonical_account_id.to_owned(),
        identity_kind: UsageIdentityKindV1::ProviderAccountId,
        rank,
        display_label: format!("account {canonical_account_id}"),
        plan_label: Some("Pro".to_owned()),
        status_label: None,
        lifecycle: UsageLifecycleV1::Available,
        freshness: freshness(),
        provenance_count: 1,
        windows: Vec::new(),
        metric_groups,
        credential_expires_at_epoch: None,
        issues: Vec::new(),
    }
}

fn freshness() -> UsageFreshnessV1 {
    UsageFreshnessV1 {
        generation: 1,
        phase: UsageFreshnessPhaseV1::Current,
        last_good_at_epoch: Some(1_800_000_000),
        retry_at_epoch: None,
        is_stale: false,
    }
}

fn window_group(group_id: &str, rank: u32, period: UsageCalendarPeriodV1) -> UsageMetricGroupV1 {
    UsageMetricGroupV1 {
        group_id: group_id.to_owned(),
        rank,
        kind: UsageMetricGroupKindV1::Window,
        label: format!("quota group {group_id}"),
        scope: UsageMetricScopeV1 {
            model: Some("gpt-5".to_owned()),
            ..UsageMetricScopeV1::default()
        },
        observed_at_epoch: Some(1_800_000_001),
        fetched_at_epoch: 1_800_000_002,
        last_success_at_epoch: Some(1_800_000_001),
        phase: UsageFreshnessPhaseV1::Current,
        is_stale: false,
        quota_state: UsageQuotaStateV1::Available,
        value: UsageMetricValueV1::Window {
            remaining_percent: Some(UsagePercent::new(64).expect("valid percent")),
            remaining_raw_percent: Some(64),
            used_percent: None,
            used_raw_percent: None,
            period: UsageMetricPeriodV1::Calendar {
                granularity: period,
            },
            unit: Some("requests".to_owned()),
        },
        reset_at_epoch: Some(1_800_100_000),
        renews_at_epoch: None,
        issues: Vec::new(),
    }
}

fn open_runtime(data_dir: &Path, projection: UsageProjectionV1) -> HostUsageProjectionRuntime {
    HostUsageProjectionRuntime::open(
        projection,
        HostUsageProjectionConfig::under_data_dir(data_dir),
    )
    .expect("open projection runtime")
}

#[test]
fn projection_presentation_keeps_canonical_identity_and_typed_detail() {
    let dir = tempfile::tempdir().expect("tempdir");
    let daily = window_group("daily", 0, UsageCalendarPeriodV1::Daily);
    let weekly = window_group("weekly", 1, UsageCalendarPeriodV1::Weekly);
    let canonical_id = "broker-canonical-openai-account";
    let publication = projection(
        "broker-a",
        1,
        vec![provider(
            "openai",
            0,
            vec![account(canonical_id, 0, vec![daily, weekly])],
        )],
    );

    let runtime = open_runtime(dir.path(), publication);
    let inventory = runtime
        .account_inventory(Some("codex"))
        .expect("account inventory");
    assert_eq!(inventory.len(), 1);
    assert_eq!(inventory[0].account.canonical_account_id, canonical_id);
    assert!(inventory[0].selected);
    assert_eq!(inventory[0].provider.provider_id, "openai");
    assert_eq!(
        inventory[0].account.metric_groups[1].scope.model.as_deref(),
        Some("gpt-5")
    );

    let presented = runtime
        .provider_presentation("codex")
        .expect("provider presentation");
    assert_eq!(presented.provider_id, "openai");
    assert_eq!(presented.detail_metric_groups.len(), 2);
    assert_eq!(
        presented
            .glance_metric_group
            .expect("weekly glance group")
            .group_id,
        "weekly"
    );
    assert!(matches!(
        presented.selected_account,
        HostUsageProjectionSelectedAccount::Available {
            canonical_account_id,
            ..
        } if canonical_account_id == canonical_id
    ));
}

#[test]
fn amp_glance_uses_typed_daily_period_and_not_provider_label_text() {
    let dir = tempfile::tempdir().expect("tempdir");
    let publication = projection(
        "broker-a",
        1,
        vec![provider(
            "amp",
            0,
            vec![account(
                "amp-canonical-id",
                0,
                vec![
                    window_group("amp-week", 0, UsageCalendarPeriodV1::Weekly),
                    window_group("amp-day", 1, UsageCalendarPeriodV1::Daily),
                ],
            )],
        )],
    );
    let runtime = open_runtime(dir.path(), publication);
    let presented = runtime
        .provider_presentation("amp")
        .expect("Amp presentation");

    assert_eq!(
        presented
            .glance_metric_group
            .expect("daily Amp glance")
            .group_id,
        "amp-day"
    );
}

#[test]
fn removed_canonical_selection_stays_unavailable_and_persisted() {
    let dir = tempfile::tempdir().expect("tempdir");
    let first = projection(
        "broker-a",
        1,
        vec![provider(
            "openai",
            0,
            vec![
                account("canonical-a", 0, Vec::new()),
                account("canonical-b", 1, Vec::new()),
            ],
        )],
    );
    let mut runtime = open_runtime(dir.path(), first);
    runtime
        .set_selected_account("codex", Some("canonical-b"))
        .expect("select second canonical account");

    runtime
        .apply_publication(projection(
            "broker-a",
            2,
            vec![provider(
                "openai",
                0,
                vec![
                    account("canonical-a", 0, Vec::new()),
                    account("canonical-c", 1, Vec::new()),
                ],
            )],
        ))
        .expect("apply complete publication");
    let presented = runtime
        .provider_presentation("codex")
        .expect("provider presentation");
    assert!(matches!(
        presented.selected_account,
        HostUsageProjectionSelectedAccount::Unavailable {
            canonical_account_id: "canonical-b"
        }
    ));
    assert!(presented.detail_metric_groups.is_empty());
    assert!(
        runtime
            .account_inventory(Some("codex"))
            .expect("account inventory")
            .iter()
            .all(|entry| !entry.selected)
    );

    let reopened = open_runtime(dir.path(), runtime.projection().clone());
    assert!(matches!(
        reopened
            .provider_presentation("codex")
            .expect("reopened presentation")
            .selected_account,
        HostUsageProjectionSelectedAccount::Unavailable {
            canonical_account_id: "canonical-b"
        }
    ));
}

#[test]
fn publication_fence_rejects_same_instance_regressions_and_allows_restart() {
    let dir = tempfile::tempdir().expect("tempdir");
    let initial = projection("broker-a", 5, vec![provider("openai", 0, Vec::new())]);
    let mut runtime = open_runtime(dir.path(), initial.clone());

    assert!(
        runtime
            .apply_publication(projection(
                "broker-a",
                4,
                vec![provider("openai", 0, Vec::new())],
            ))
            .is_err()
    );
    assert_eq!(runtime.projection(), &initial);

    assert!(
        runtime
            .apply_publication(projection(
                "broker-a",
                5,
                vec![provider("anthropic", 0, Vec::new())],
            ))
            .is_err()
    );
    runtime
        .apply_publication(initial)
        .expect("identical publication is idempotent");

    runtime
        .apply_publication(projection(
            "broker-b",
            1,
            vec![provider("openai", 0, Vec::new())],
        ))
        .expect("new broker incarnation may restart generation");
    assert_eq!(runtime.projection().broker_instance_id, "broker-b");
    assert_eq!(runtime.projection().broker_generation, 1);
}

#[test]
fn enabled_surface_filter_preserves_broker_provider_order() {
    let dir = tempfile::tempdir().expect("tempdir");
    let publication = projection(
        "broker-a",
        1,
        vec![
            provider("anthropic", 0, vec![account("claude-id", 0, Vec::new())]),
            provider("openai", 1, vec![account("codex-id", 0, Vec::new())]),
        ],
    );
    let mut config = HostUsageProjectionConfig::under_data_dir(dir.path());
    config.enabled_surface_ids = vec!["codex".to_owned()];
    let runtime = HostUsageProjectionRuntime::open(publication, config).expect("open");

    let inventory = runtime.account_inventory(None).expect("enabled inventory");
    assert_eq!(inventory.len(), 1);
    assert_eq!(inventory[0].surface_id, "codex");
    assert_eq!(inventory[0].account.canonical_account_id, "codex-id");
    assert!(
        runtime
            .account_inventory(Some("claude"))
            .unwrap()
            .is_empty()
    );
    let error = runtime.provider_presentation("claude").unwrap_err();
    assert_eq!(error, "surface disabled: claude");
}

#[test]
fn unresolved_provider_state_stays_separate_from_account_inventory() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut publication = projection("broker-a", 1, Vec::new());
    publication.unresolved.push(UsageUnresolvedV1 {
        provider_id: "openai".to_owned(),
        capability_id: "opaque-capability".to_owned(),
        configuration_count: 1,
        state: UsageLifecycleV1::NeedsLogin,
        issues: Vec::new(),
    });
    let runtime = open_runtime(dir.path(), publication);

    assert!(
        runtime
            .account_inventory(Some("codex"))
            .expect("account inventory")
            .is_empty()
    );
    let presented = runtime
        .provider_presentation("codex")
        .expect("provider presentation");
    assert!(presented.provider.is_none());
    assert_eq!(presented.unresolved_capabilities.len(), 1);
    assert_eq!(
        presented.unresolved_capabilities[0].state,
        UsageLifecycleV1::NeedsLogin
    );
}
