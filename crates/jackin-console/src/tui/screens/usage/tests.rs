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

#[test]
fn projection_includes_unresolved_accounts_and_groups_by_provider() {
    use jackin_protocol::usage_broker::{
        UsageIssueRecoverabilityV1, UsageIssueScopeV1, UsageIssueV1, UsageLifecycleV1,
        UsageProjectionRefreshStateV1, UsageProjectionSchemaV1, UsageProjectionV1,
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
        providers: vec![jackin_protocol::usage_broker::UsageProviderV1 {
            provider_id: "openai".to_owned(),
            display_name: "OpenAI".to_owned(),
            rank: 0,
            membership_state: jackin_protocol::usage_broker::UsageMembershipStateV1::Current,
            freshness: jackin_protocol::usage_broker::UsageFreshnessV1 {
                generation: 1,
                phase: jackin_protocol::usage_broker::UsageFreshnessPhaseV1::Current,
                last_good_at_epoch: None,
                retry_at_epoch: None,
                is_stale: false,
            },
            accounts: vec![jackin_protocol::usage_broker::UsageAccountV1 {
                canonical_account_id: "a".to_owned(),
                identity_kind:
                    jackin_protocol::usage_broker::UsageIdentityKindV1::ProviderStableHandle,
                rank: 0,
                display_label: "work@example.test".to_owned(),
                plan_label: None,
                status_label: None,
                lifecycle: UsageLifecycleV1::Available,
                freshness: jackin_protocol::usage_broker::UsageFreshnessV1 {
                    generation: 1,
                    phase: jackin_protocol::usage_broker::UsageFreshnessPhaseV1::Current,
                    last_good_at_epoch: None,
                    retry_at_epoch: None,
                    is_stale: false,
                },
                provenance_count: 1,
                windows: Vec::new(),
                issues: Vec::new(),
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
    assert_eq!(state.accounts[2].provider, "Anthropic / Claude");
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

#[test]
fn render_detail_overview_renders_all_windows_and_scrolling() {
    use crate::tui::state::ManagerState;
    use ratatui::{Terminal, backend::TestBackend};

    let config = jackin_config::AppConfig::default();
    let cwd = std::path::Path::new("/test");
    let mut manager = ManagerState::from_config(&config, cwd);

    let state = UsageScreenState {
        accounts: vec![
            super::UsageAccount {
                provider: "OpenAI".to_owned(),
                account: "work".to_owned(),
                status: "available".to_owned(),
                windows: vec![
                    super::UsageWindow {
                        label: "5h session".to_owned(),
                        value: "10% left".to_owned(),
                        reset: "resets in 2h".to_owned(),
                        remaining_percent: Some(10),
                    },
                    super::UsageWindow {
                        label: "weekly".to_owned(),
                        value: "80% left".to_owned(),
                        reset: "resets in 5d".to_owned(),
                        remaining_percent: Some(80),
                    },
                ],
            },
            super::UsageAccount {
                provider: "Anthropic / Claude".to_owned(),
                account: "Unresolved (claude:key)".to_owned(),
                status: "needs login · authentication required".to_owned(),
                windows: Vec::new(),
            },
        ],
        selected: 0,
        detail: false,
        scroll: 0,
        notice: Some("1 configured capability(s) unresolved".to_owned()),
    };
    manager.usage_screen = Some(state);

    let backend = TestBackend::new(80, 25);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| super::render_detail(f, f.area(), &manager))
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    let text = (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert!(text.contains("OpenAI · work"));
    assert!(text.contains("5h session"));
    assert!(text.contains("10% left · resets in 2h"));
    assert!(text.contains("weekly"));
    assert!(text.contains("80% left · resets in 5d"));
    assert!(text.contains("Anthropic / Claude · Unresolved (claude:key)"));
    assert!(text.contains("needs login · authentication required"));
    assert!(text.contains("1 configured capability(s) unresolved"));
}
