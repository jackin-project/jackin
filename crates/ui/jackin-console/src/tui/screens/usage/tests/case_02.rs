// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn relative_labels_hold_at_fixed_epoch_boundaries() {
    let now = TEST_NOW_EPOCH;

    for (offset, expected) in [
        (0, "in under a minute"),
        (59, "in under a minute"),
        (60, "in 1m"),
        (3_599, "in 59m"),
        (3_600, "in 1h"),
        (86_399, "in 23h"),
        (86_400, "in 1d"),
    ] {
        assert_eq!(relative_time_label(now, now + offset), expected);
    }
    for (offset, expected) in [
        (1, "just now"),
        (59, "just now"),
        (60, "1m ago"),
        (3_599, "59m ago"),
        (3_600, "1h ago"),
        (86_399, "23h ago"),
        (86_400, "1d ago"),
    ] {
        assert_eq!(relative_time_label(now, now - offset), expected);
    }

    assert_eq!(
        credential_expiry_label(now, now),
        "expires in under a minute"
    );
    assert_eq!(credential_expiry_label(now, now - 1), "expired just now");
}

#[test]
fn render_at_shares_one_epoch_between_list_and_detail() {
    use ratatui::{Terminal, backend::TestBackend};

    let now = TEST_NOW_EPOCH;
    let mut account = test_account("openai", "work-id", "work");
    account.provider = "OpenAI".to_owned();
    account.last_good_at_epoch = Some(now - 60);
    account.retry_at_epoch = Some(now + 60);
    account.credential_expires_at_epoch = Some(now + 60);
    let manager = manager_with_usage(UsageScreenState {
        accounts: vec![account],
        selected: 1,
        selected_id: Some("openai:work-id".to_owned()),
        ..UsageScreenState::default()
    });

    let backend = TestBackend::new(200, 40);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| render_at(frame, frame.area(), &manager, now))
        .unwrap();
    let text = backend_text(&terminal);

    assert!(
        text.contains("updated 1m ago"),
        "list freshness drifted:\n{text}"
    );
    assert!(
        text.contains("Freshness updated 1m ago · retry in 1m"),
        "detail freshness drifted:\n{text}"
    );
    assert!(
        text.contains("Credential expires in 1m"),
        "credential expiry drifted:\n{text}"
    );
}

#[test]
fn projection_keeps_canonical_ids_and_freshness() {
    use jackin_protocol::usage_broker::{
        UsageAccountV1, UsageFreshnessPhaseV1, UsageFreshnessV1, UsageIdentityKindV1,
        UsageLifecycleV1, UsageLimitWindowV1, UsageMembershipStateV1, UsagePercent,
        UsageProjectionRefreshStateV1, UsageProjectionSchemaV1, UsageProjectionV1, UsageProviderV1,
        UsageQuotaStateV1, UsageWindowCategoryV1,
    };

    let projection = UsageProjectionV1 {
        schema_version: UsageProjectionSchemaV1,
        projection_id: "p".to_owned(),
        generated_at_epoch: 1_800_000_000,
        discovery_revision: "d".to_owned(),
        broker_instance_id: "b".to_owned(),
        broker_generation: 1,
        refresh_state: UsageProjectionRefreshStateV1::Idle,
        providers: vec![UsageProviderV1 {
            provider_id: "openai".to_owned(),
            display_name: "OpenAI".to_owned(),
            rank: 0,
            membership_state: UsageMembershipStateV1::Current,
            freshness: UsageFreshnessV1 {
                generation: 1,
                phase: UsageFreshnessPhaseV1::Current,
                last_good_at_epoch: None,
                retry_at_epoch: None,
                is_stale: false,
            },
            accounts: vec![UsageAccountV1 {
                canonical_account_id: "canon-1".to_owned(),
                identity_kind: UsageIdentityKindV1::ProviderStableHandle,
                rank: 0,
                display_label: "work@example.test".to_owned(),
                plan_label: None,
                status_label: None,
                lifecycle: UsageLifecycleV1::Available,
                freshness: UsageFreshnessV1 {
                    generation: 1,
                    phase: UsageFreshnessPhaseV1::Stale,
                    last_good_at_epoch: Some(1_799_000_000),
                    retry_at_epoch: None,
                    is_stale: true,
                },
                provenance_count: 1,
                windows: vec![UsageLimitWindowV1 {
                    window_id: "weekly".to_owned(),
                    rank: 0,
                    category: UsageWindowCategoryV1::LongRange,
                    label: "weekly".to_owned(),
                    value_label: "73% left".to_owned(),
                    reset_label: "resets tomorrow".to_owned(),
                    remaining_percent: None,
                    used_percent: Some(UsagePercent::new(27).expect("valid percent")),
                    reset_at_epoch: Some(1_800_100_000),
                    quota_state: UsageQuotaStateV1::Available,
                    pace_label: None,
                    runs_out_label: None,
                    remaining_raw_percent: None,
                    used_raw_percent: Some(27),
                }],
                issues: Vec::new(),
                metric_groups: Vec::new(),
                credential_expires_at_epoch: None,
            }],
            issues: Vec::new(),
        }],
        unresolved: Vec::new(),
        issues: Vec::new(),
    };

    let state = UsageScreenState::from_projection(&projection);
    assert_eq!(state.accounts.len(), 1);
    let account = &state.accounts[0];
    assert_eq!(account.provider_id, "openai");
    assert_eq!(account.canonical_account_id, "canon-1");
    assert!(!account.unresolved);
    assert_eq!(account.stable_id(), "openai:canon-1");
    assert_eq!(account.lifecycle, UsageLifecycleV1::Available);
    assert_eq!(account.freshness_phase, UsageFreshnessPhaseV1::Stale);
    assert_eq!(account.last_good_at_epoch, Some(1_799_000_000));
    assert!(account.is_stale);
    assert_eq!(account.status, "stale");
    assert_eq!(account.windows[0].window_id, "weekly");
    assert_eq!(account.windows[0].remaining_percent, None);
    assert_eq!(account.windows[0].used_percent, Some(27));
    assert_eq!(account.windows[0].meter_percent(), Some(73));
    assert_eq!(account.windows[0].reset_at_epoch, Some(1_800_100_000));
    assert_eq!(state.generated_at_epoch, Some(1_800_000_000));
}

#[test]
fn projection_keeps_provider_accounts_once_and_preserves_window_order() {
    use jackin_protocol::usage_broker::{
        UsageAccountV1, UsageFreshnessPhaseV1, UsageFreshnessV1, UsageIdentityKindV1,
        UsageLifecycleV1, UsageLimitWindowV1, UsageMembershipStateV1, UsagePercent,
        UsageProjectionRefreshStateV1, UsageProjectionSchemaV1, UsageProjectionV1, UsageProviderV1,
        UsageQuotaStateV1, UsageWindowCategoryV1,
    };

    let projection = UsageProjectionV1 {
        schema_version: UsageProjectionSchemaV1,
        projection_id: "p".to_owned(),
        generated_at_epoch: 0,
        discovery_revision: "d".to_owned(),
        broker_instance_id: "b".to_owned(),
        broker_generation: 1,
        refresh_state: UsageProjectionRefreshStateV1::Idle,
        providers: vec![UsageProviderV1 {
            provider_id: "openai".to_owned(),
            display_name: "OpenAI".to_owned(),
            rank: 0,
            membership_state: UsageMembershipStateV1::Current,
            freshness: UsageFreshnessV1 {
                generation: 1,
                phase: UsageFreshnessPhaseV1::Current,
                last_good_at_epoch: None,
                retry_at_epoch: None,
                is_stale: false,
            },
            accounts: vec![UsageAccountV1 {
                canonical_account_id: "a".to_owned(),
                identity_kind: UsageIdentityKindV1::ProviderStableHandle,
                rank: 0,
                display_label: "work@example.test".to_owned(),
                plan_label: None,
                status_label: None,
                lifecycle: UsageLifecycleV1::Available,
                freshness: UsageFreshnessV1 {
                    generation: 1,
                    phase: UsageFreshnessPhaseV1::Current,
                    last_good_at_epoch: None,
                    retry_at_epoch: None,
                    is_stale: false,
                },
                provenance_count: 1,
                windows: vec![UsageLimitWindowV1 {
                    window_id: "weekly".to_owned(),
                    rank: 0,
                    category: UsageWindowCategoryV1::LongRange,
                    label: "weekly".to_owned(),
                    value_label: "73% left".to_owned(),
                    reset_label: "resets tomorrow".to_owned(),
                    remaining_percent: Some(UsagePercent::new(73).expect("valid percent")),
                    used_percent: None,
                    reset_at_epoch: None,
                    quota_state: UsageQuotaStateV1::Available,
                    pace_label: None,
                    runs_out_label: None,
                    remaining_raw_percent: Some(73),
                    used_raw_percent: None,
                }],
                issues: Vec::new(),
                metric_groups: Vec::new(),
                credential_expires_at_epoch: None,
            }],
            issues: Vec::new(),
        }],
        unresolved: Vec::new(),
        issues: Vec::new(),
    };

    let state = UsageScreenState::from_projection(&projection);
    assert_eq!(state.accounts.len(), 1);
    assert_eq!(state.accounts[0].provider, "OpenAI");
    assert_eq!(state.accounts[0].windows[0].label, "weekly");
    assert_eq!(state.accounts[0].windows[0].remaining_percent, Some(73));
}

#[test]
fn projection_includes_unresolved_accounts_and_groups_by_provider() {
    use jackin_protocol::usage_broker::{
        UsageAccountV1, UsageFreshnessV1, UsageIdentityKindV1, UsageIssueRecoverabilityV1,
        UsageIssueScopeV1, UsageIssueV1, UsageLifecycleV1, UsageMembershipStateV1,
        UsageProjectionRefreshStateV1, UsageProjectionSchemaV1, UsageProjectionV1, UsageProviderV1,
        UsageUnresolvedV1,
    };

    let projection = UsageProjectionV1 {
        schema_version: UsageProjectionSchemaV1,
        projection_id: "p".to_owned(),
        generated_at_epoch: 0,
        discovery_revision: "d".to_owned(),
        broker_instance_id: "b".to_owned(),
        broker_generation: 1,
        refresh_state: UsageProjectionRefreshStateV1::Idle,
        providers: vec![UsageProviderV1 {
            provider_id: "openai".to_owned(),
            display_name: "OpenAI".to_owned(),
            rank: 0,
            membership_state: UsageMembershipStateV1::Current,
            freshness: UsageFreshnessV1 {
                generation: 1,
                phase: UsageFreshnessPhaseV1::Current,
                last_good_at_epoch: None,
                retry_at_epoch: None,
                is_stale: false,
            },
            accounts: vec![UsageAccountV1 {
                canonical_account_id: "a".to_owned(),
                identity_kind: UsageIdentityKindV1::ProviderStableHandle,
                rank: 0,
                display_label: "work@example.test".to_owned(),
                plan_label: None,
                status_label: None,
                lifecycle: UsageLifecycleV1::Available,
                freshness: UsageFreshnessV1 {
                    generation: 1,
                    phase: UsageFreshnessPhaseV1::Current,
                    last_good_at_epoch: None,
                    retry_at_epoch: None,
                    is_stale: false,
                },
                provenance_count: 1,
                windows: Vec::new(),
                issues: Vec::new(),
                metric_groups: Vec::new(),
                credential_expires_at_epoch: None,
            }],
            issues: Vec::new(),
        }],
        unresolved: vec![
            UsageUnresolvedV1 {
                provider_id: "anthropic".to_owned(),
                capability_id: "anthropic:key".to_owned(),
                configuration_count: 1,
                state: UsageLifecycleV1::NeedsLogin,
                issues: vec![UsageIssueV1 {
                    code: "auth_required".to_owned(),
                    scope: UsageIssueScopeV1::Account,
                    recoverability: UsageIssueRecoverabilityV1::ActionRequired,
                    message: "authentication required".to_owned(),
                    retry_at_epoch: None,
                }],
            },
            UsageUnresolvedV1 {
                provider_id: "openai".to_owned(),
                capability_id: "openai:second".to_owned(),
                configuration_count: 1,
                state: UsageLifecycleV1::NeedsLogin,
                issues: Vec::new(),
            },
        ],
        issues: Vec::new(),
    };

    let state = UsageScreenState::from_projection(&projection);
    assert_eq!(state.accounts.len(), 3);
    assert_eq!(state.accounts[0].provider, "OpenAI");
    assert_eq!(state.accounts[0].account, "work@example.test");
    assert_eq!(state.accounts[1].provider, "OpenAI");
    assert_eq!(state.accounts[1].account, "Unresolved (openai:second)");
    assert_eq!(state.accounts[1].status, "needs login");
    assert!(state.accounts[1].unresolved);
    assert_eq!(state.accounts[1].provider_id, "openai");
    assert_eq!(state.accounts[1].canonical_account_id, "openai:second");
    assert_eq!(state.accounts[1].stable_id(), "openai:openai:second");
    assert_eq!(state.accounts[2].provider, "Anthropic");
    assert_eq!(state.accounts[2].account, "Unresolved (anthropic:key)");
    assert_eq!(
        state.accounts[2].status,
        "needs login · authentication required"
    );
    assert_eq!(
        state.notice,
        Some("2 configured capability(s) unresolved".to_owned())
    );
}
