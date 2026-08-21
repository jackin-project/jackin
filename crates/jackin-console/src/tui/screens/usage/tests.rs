// SPDX-FileCopyrightText: 2026 Alexey Zhokov
// SPDX-License-Identifier: Apache-2.0

use super::{UsageScreenState, meter_line};

#[test]
fn usage_meter_scales_to_remaining_percentage() {
    assert_eq!(meter_line(10, Some(50)), "  █████░░░░░");
    assert_eq!(meter_line(10, Some(0)), "  ░░░░░░░░░░");
    assert_eq!(meter_line(10, Some(100)), "  ██████████");
}

#[test]
fn usage_selection_stays_in_bounds() {
    let mut state = UsageScreenState {
        accounts: vec![
            super::UsageAccount {
                provider: "OpenAI".to_owned(),
                account: "a".to_owned(),
                status: "available".to_owned(),
                windows: Vec::new(),
            },
            super::UsageAccount {
                provider: "Anthropic".to_owned(),
                account: "b".to_owned(),
                status: "available".to_owned(),
                windows: Vec::new(),
            },
        ],
        ..UsageScreenState::default()
    };
    state.move_selection(1);
    state.move_selection(1);
    assert_eq!(state.selected, 2);
    state.move_selection(-9);
    assert_eq!(state.selected, 0);
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
                }],
                issues: Vec::new(),
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
