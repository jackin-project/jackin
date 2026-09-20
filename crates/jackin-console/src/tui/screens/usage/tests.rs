// SPDX-FileCopyrightText: 2026 Alexey Zhokov
// SPDX-License-Identifier: Apache-2.0

use std::time::{Duration, Instant};

use super::{
    USAGE_HEARTBEAT_INTERVAL, UsageAccount, UsageScreenState, UsageWindow, freshness_age_label,
    meter_line,
};
use jackin_protocol::usage_broker::{
    UsageFreshnessPhaseV1, UsageIdentityKindV1, UsageLifecycleV1, UsageQuotaStateV1,
    UsageWindowCategoryV1,
};

fn test_window(label: &str, remaining: Option<u8>) -> UsageWindow {
    UsageWindow {
        window_id: format!("{label}-id"),
        rank: 0,
        category: UsageWindowCategoryV1::LongRange,
        label: label.to_owned(),
        value: format!("{label} value"),
        reset: "resets soon".to_owned(),
        remaining_percent: remaining,
        remaining_raw_percent: remaining.map(i32::from),
        used_percent: None,
        used_raw_percent: None,
        reset_at_epoch: Some(1_800_000_000),
        quota_state: UsageQuotaStateV1::Available,
        pace_label: None,
        runs_out_label: None,
    }
}

fn test_account(provider_id: &str, account_id: &str, label: &str) -> UsageAccount {
    UsageAccount {
        provider_id: provider_id.to_owned(),
        canonical_account_id: account_id.to_owned(),
        unresolved: false,
        provider: provider_id.to_owned(),
        account: label.to_owned(),
        status: "available".to_owned(),
        lifecycle: UsageLifecycleV1::Available,
        freshness_phase: UsageFreshnessPhaseV1::Current,
        last_good_at_epoch: Some(1_799_999_000),
        retry_at_epoch: None,
        is_stale: false,
        identity_kind: Some(UsageIdentityKindV1::ProviderStableHandle),
        plan_label: None,
        credential_expires_at_epoch: None,
        issues: Vec::new(),
        provider_issues: Vec::new(),
        windows: vec![test_window("weekly", Some(73))],
        metric_groups: Vec::new(),
    }
}

#[test]
fn usage_meter_scales_to_remaining_percentage() {
    assert_eq!(meter_line(10, Some(50)), Some("  █████░░░░░".to_owned()));
    assert_eq!(meter_line(10, Some(0)), Some("  ░░░░░░░░░░".to_owned()));
    assert_eq!(meter_line(10, Some(100)), Some("  ██████████".to_owned()));
}

#[test]
fn usage_meter_renders_no_bar_for_unknown_percent() {
    assert_eq!(meter_line(10, None), None);
}

#[test]
fn usage_window_mirrors_used_percent_to_meter() {
    let window = UsageWindow {
        remaining_percent: None,
        used_percent: Some(30),
        ..test_window("weekly", None)
    };
    assert_eq!(window.meter_percent(), Some(70));
    let unknown = test_window("weekly", None);
    assert_eq!(unknown.meter_percent(), None);
}

#[test]
fn usage_selection_stays_in_bounds() {
    let mut state = UsageScreenState {
        accounts: vec![
            test_account("openai", "a", "a"),
            test_account("anthropic", "b", "b"),
        ],
        ..UsageScreenState::default()
    };
    state.move_selection(1);
    state.move_selection(1);
    assert_eq!(state.selected, 2);
    assert_eq!(state.selected_id, Some("anthropic:b".to_owned()));
    state.move_selection(-9);
    assert_eq!(state.selected, 0);
    assert_eq!(state.selected_id, None);
}

#[test]
fn apply_refresh_preserves_selection_across_rename_and_reorder() {
    let now = Instant::now();
    let mut state = UsageScreenState::open_with_snapshot(
        vec![
            test_account("openai", "a", "work"),
            test_account("anthropic", "b", "personal"),
        ],
        None,
    );
    state.move_selection(1);
    assert_eq!(state.selected_id, Some("openai:a".to_owned()));

    let mut renamed = test_account("openai", "a", "work-renamed");
    renamed.provider = "OpenAI Renamed".to_owned();
    state.apply_refresh(
        vec![test_account("anthropic", "b", "personal"), renamed],
        None,
        now,
    );
    assert_eq!(state.selected, 2);
    assert_eq!(state.selected_account().unwrap().account, "work-renamed");
    assert_eq!(state.selected_id, Some("openai:a".to_owned()));
    assert!(state.notice.is_none());
    assert_eq!(state.last_refresh_at, Some(now));
}

#[test]
fn apply_refresh_removed_selection_falls_back_to_overview_with_notice() {
    let now = Instant::now();
    let mut state = UsageScreenState::open_with_snapshot(
        vec![
            test_account("openai", "a", "work"),
            test_account("anthropic", "b", "personal"),
        ],
        None,
    );
    state.move_selection(2);
    assert_eq!(state.selected_id, Some("anthropic:b".to_owned()));

    state.apply_refresh(vec![test_account("openai", "a", "work")], None, now);
    assert_eq!(state.selected, 0);
    assert_eq!(state.selected_id, None);
    assert_eq!(
        state.notice,
        Some("Previously selected account unavailable; showing Overview".to_owned())
    );
}

#[test]
fn apply_refresh_error_advances_timer_without_moving_selection() {
    let now = Instant::now();
    let mut state =
        UsageScreenState::open_with_snapshot(vec![test_account("openai", "a", "work")], None);
    state.move_selection(1);
    state.apply_refresh_error("Usage unavailable: boom".to_owned(), now);
    assert_eq!(state.selected, 1);
    assert_eq!(state.selected_id, Some("openai:a".to_owned()));
    assert_eq!(state.notice, Some("Usage unavailable: boom".to_owned()));
    assert_eq!(state.last_refresh_at, Some(now));
    assert!(!state.refresh_due);
}

#[test]
fn heartbeat_due_only_after_first_completion() {
    let state = UsageScreenState::open_with_snapshot(Vec::new(), None);
    assert!(state.refresh_due);
    assert!(!state.heartbeat_due(Instant::now()));

    let mut state = state;
    let completed = Instant::now()
        .checked_sub(USAGE_HEARTBEAT_INTERVAL + Duration::from_secs(1))
        .expect("heartbeat interval fits in uptime");
    state.apply_refresh(Vec::new(), None, completed);
    assert!(state.heartbeat_due(Instant::now()));

    let mut fresh = UsageScreenState::open_with_snapshot(Vec::new(), None);
    fresh.apply_refresh(Vec::new(), None, Instant::now());
    assert!(!fresh.heartbeat_due(Instant::now()));
}

#[test]
fn poll_refresh_delivers_ready_outcome_and_clears_in_flight() {
    let mut state = UsageScreenState::open_with_snapshot(Vec::new(), None);
    assert!(!state.refresh_in_flight());
    let plan = state
        .next_refresh_plan_if_due(Instant::now())
        .expect("open marks a refresh due");
    state.begin_refresh(crate::tui::runtime::ready_blocking_subscription((
        plan.generation,
        Ok((
            vec![test_account("openai", "a", "work")],
            Some("n".to_owned()),
        )),
    )));
    assert!(state.refresh_in_flight());
    let outcome = state.poll_refresh().expect("ready outcome");
    let (accounts, notice) = outcome.expect("ok outcome");
    assert_eq!(accounts.len(), 1);
    assert_eq!(notice, Some("n".to_owned()));
    assert!(!state.refresh_in_flight());
}

#[test]
fn poll_refresh_drops_stale_generations() {
    let mut state = UsageScreenState::open_with_snapshot(Vec::new(), None);
    let plan = state
        .next_refresh_plan_if_due(Instant::now())
        .expect("open marks a refresh due");
    state.begin_refresh(crate::tui::runtime::ready_blocking_subscription((
        plan.generation.wrapping_add(1),
        Ok((vec![test_account("openai", "a", "work")], None)),
    )));
    assert!(state.poll_refresh().is_none(), "stale outcome dropped");
    assert!(!state.refresh_in_flight());
    assert!(state.accounts.is_empty());
}

#[test]
fn refresh_plan_joins_in_flight_work_without_queueing() {
    let mut state = UsageScreenState::open_with_snapshot(Vec::new(), None);
    let plan = state
        .next_refresh_plan_if_due(Instant::now())
        .expect("open marks a refresh due");
    assert!(!plan.force);
    state.begin_refresh(crate::tui::runtime::ready_blocking_subscription((
        plan.generation,
        Ok((Vec::new(), None)),
    )));
    state.refresh_due = true;
    state.force_refresh_pending = true;
    assert!(
        state.next_refresh_plan_if_due(Instant::now()).is_none(),
        "in-flight refresh is joined, never duplicated"
    );
    assert!(!state.refresh_due);
}

#[test]
fn refresh_plan_carries_force_only_for_manual_refresh() {
    let mut state = UsageScreenState::open_with_snapshot(Vec::new(), None);
    let open = state
        .next_refresh_plan_if_due(Instant::now())
        .expect("open marks a refresh due");
    assert!(!open.force, "open subscribes without forcing");
    state.apply_refresh(Vec::new(), None, Instant::now());
    assert!(
        state.next_refresh_plan_if_due(Instant::now()).is_none(),
        "fresh completion is not due"
    );

    state.refresh_due = true;
    state.force_refresh_pending = true;
    let manual = state
        .next_refresh_plan_if_due(Instant::now())
        .expect("manual refresh is due");
    assert!(manual.force);
    assert_eq!(manual.generation, open.generation.wrapping_add(1));
    assert!(!state.force_refresh_pending, "force is consumed on claim");

    state.apply_refresh(Vec::new(), None, Instant::now());
    let completed = Instant::now()
        .checked_sub(USAGE_HEARTBEAT_INTERVAL + Duration::from_secs(1))
        .expect("heartbeat interval fits in uptime");
    state.apply_refresh(Vec::new(), None, completed);
    let heartbeat = state
        .next_refresh_plan_if_due(Instant::now())
        .expect("heartbeat is due");
    assert!(!heartbeat.force, "periodic refresh honors broker cadence");
}

#[test]
fn screen_clone_drops_in_flight_refresh_but_keeps_value_state() {
    let mut state =
        UsageScreenState::open_with_snapshot(vec![test_account("openai", "a", "work")], None);
    let plan = state
        .next_refresh_plan_if_due(Instant::now())
        .expect("open marks a refresh due");
    state.begin_refresh(crate::tui::runtime::ready_blocking_subscription((
        plan.generation,
        Ok((Vec::new(), None)),
    )));
    let cloned = state.clone();
    assert!(state.refresh_in_flight());
    assert!(!cloned.refresh_in_flight());
    assert_eq!(cloned.accounts, state.accounts);
    assert_eq!(cloned.refresh_generation, state.refresh_generation);
    assert_eq!(cloned, state);

    let mut diverged = cloned.clone();
    diverged.refresh_generation = diverged.refresh_generation.wrapping_add(1);
    assert_ne!(diverged, cloned);
}

#[test]
fn open_with_snapshot_marks_refresh_due() {
    let state =
        UsageScreenState::open_with_snapshot(vec![test_account("openai", "a", "work")], None);
    assert!(state.refresh_due);
    assert_eq!(state.selected, 0);
    assert_eq!(state.selected_id, None);
}

#[test]
fn manual_refresh_key_marks_refresh_due() {
    use crate::tui::state::ManagerState;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    let config = jackin_config::AppConfig::default();
    let mut manager = ManagerState::from_config(&config, std::path::Path::new("/test"));
    manager.usage.screen = Some(UsageScreenState::open_with_snapshot(Vec::new(), None));
    manager
        .usage
        .screen
        .as_mut()
        .unwrap()
        .apply_refresh(Vec::new(), None, Instant::now());
    assert!(!manager.usage.screen.as_ref().unwrap().refresh_due);

    let key = KeyEvent {
        code: KeyCode::Char('r'),
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: KeyEventState::empty(),
    };
    super::handle_key(&mut manager, key);
    let screen = manager.usage.screen.as_ref().unwrap();
    assert!(screen.refresh_due);
    assert!(screen.force_refresh_pending);
}

#[test]
fn freshness_age_label_covers_phases_and_ages() {
    let now = 1_800_000_000;
    let mut account = test_account("openai", "a", "work");

    account.freshness_phase = UsageFreshnessPhaseV1::Refreshing;
    assert_eq!(freshness_age_label(now, &account), "refreshing…");

    account.freshness_phase = UsageFreshnessPhaseV1::Current;
    account.last_good_at_epoch = None;
    assert_eq!(freshness_age_label(now, &account), "never updated");

    account.last_good_at_epoch = Some(now - 10);
    assert_eq!(freshness_age_label(now, &account), "updated now");
    account.last_good_at_epoch = Some(now - 300);
    assert_eq!(freshness_age_label(now, &account), "updated 5m ago");
    account.last_good_at_epoch = Some(now - 7_200);
    assert_eq!(freshness_age_label(now, &account), "updated 2h ago");
    account.last_good_at_epoch = Some(now - 172_800);
    assert_eq!(freshness_age_label(now, &account), "updated 2d ago");

    account.is_stale = true;
    account.last_good_at_epoch = Some(now - 300);
    assert_eq!(freshness_age_label(now, &account), "stale · updated 5m ago");
    account.is_stale = false;
    account.freshness_phase = UsageFreshnessPhaseV1::Stale;
    assert_eq!(freshness_age_label(now, &account), "stale · updated 5m ago");
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

#[test]
fn render_detail_overview_renders_all_windows_and_scrolling() {
    use crate::tui::state::ManagerState;
    use ratatui::{Terminal, backend::TestBackend};

    let config = jackin_config::AppConfig::default();
    let cwd = std::path::Path::new("/test");
    let mut manager = ManagerState::from_config(&config, cwd);

    let mut openai = test_account("openai", "work-id", "work");
    openai.provider = "OpenAI".to_owned();
    openai.windows = vec![
        UsageWindow {
            window_id: "session".to_owned(),
            rank: 0,
            category: UsageWindowCategoryV1::Session,
            label: "5h session".to_owned(),
            value: "10% left".to_owned(),
            reset: "resets in 2h".to_owned(),
            remaining_percent: Some(10),
            remaining_raw_percent: Some(10),
            used_percent: None,
            used_raw_percent: None,
            reset_at_epoch: None,
            quota_state: UsageQuotaStateV1::Available,
            pace_label: None,
            runs_out_label: None,
        },
        UsageWindow {
            window_id: "weekly".to_owned(),
            rank: 1,
            category: UsageWindowCategoryV1::LongRange,
            label: "weekly".to_owned(),
            value: "80% left".to_owned(),
            reset: "resets in 5d".to_owned(),
            remaining_percent: Some(80),
            remaining_raw_percent: Some(80),
            used_percent: None,
            used_raw_percent: None,
            reset_at_epoch: None,
            quota_state: UsageQuotaStateV1::Available,
            pace_label: None,
            runs_out_label: None,
        },
    ];
    let mut claude = test_account("anthropic", "claude:key", "Unresolved (claude:key)");
    claude.provider = "Anthropic / Claude".to_owned();
    claude.unresolved = true;
    claude.status = "needs login · authentication required".to_owned();
    claude.lifecycle = UsageLifecycleV1::NeedsLogin;
    claude.last_good_at_epoch = None;
    claude.windows = Vec::new();
    let state = UsageScreenState {
        accounts: vec![openai, claude],
        selected: 0,
        detail: false,
        scroll: 0,
        notice: Some("1 configured capability(s) unresolved".to_owned()),
        ..UsageScreenState::default()
    };
    manager.usage.screen = Some(state);

    let backend = TestBackend::new(80, 25);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| super::render_detail(f, f.area(), &manager))
        .unwrap();

    let text = backend_text(&terminal);

    assert!(text.contains("OpenAI · work"));
    assert!(text.contains("5h session"));
    assert!(text.contains("10% left · resets in 2h"));
    assert!(text.contains("weekly"));
    assert!(text.contains("80% left · resets in 5d"));
    assert!(text.contains("Anthropic / Claude · Unresolved (claude:key)"));
    assert!(text.contains("needs login · authentication required"));
    assert!(text.contains("1 configured capability(s) unresolved"));
}

#[test]
fn render_full_route_narrow_and_wide() {
    use crate::tui::state::ManagerState;
    use ratatui::{Terminal, backend::TestBackend};

    for (width, height) in [(40, 20), (120, 30)] {
        let config = jackin_config::AppConfig::default();
        let cwd = std::path::Path::new("/test");
        let mut manager = ManagerState::from_config(&config, cwd);
        let mut account = test_account("openai", "work-id", "work");
        account.provider = "OpenAI".to_owned();
        manager.usage.screen = Some(UsageScreenState {
            accounts: vec![account],
            selected: 0,
            notice: Some("1 configured capability(s) unresolved".to_owned()),
            ..UsageScreenState::default()
        });

        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| super::render(f, f.area(), &manager))
            .unwrap();

        let text = backend_text(&terminal);
        assert!(
            text.contains("OpenAI · work"),
            "missing account header at {width}x{height}"
        );
        // Narrow widths wrap the notice across rows; assert tokens there
        // and the full string where it fits on one row.
        if width < 80 {
            assert!(
                text.contains("unresolved"),
                "missing notice token at {width}x{height}"
            );
        } else {
            assert!(
                text.contains("1 configured capability(s) unresolved"),
                "missing notice at {width}x{height}"
            );
        }
    }
}

#[test]
fn render_detail_account_shows_freshness_and_refreshing_indicator() {
    use crate::tui::state::ManagerState;
    use ratatui::{Terminal, backend::TestBackend};

    let config = jackin_config::AppConfig::default();
    let cwd = std::path::Path::new("/test");
    let mut manager = ManagerState::from_config(&config, cwd);
    let mut account = test_account("openai", "work-id", "work");
    account.provider = "OpenAI".to_owned();
    account.last_good_at_epoch = Some(1_000_000);
    account.is_stale = true;
    let mut state = UsageScreenState {
        accounts: vec![account],
        selected: 1,
        selected_id: Some("openai:work-id".to_owned()),
        ..UsageScreenState::default()
    };
    state.begin_refresh(crate::tui::runtime::ready_blocking_subscription((
        state.refresh_generation,
        Ok((Vec::new(), None)),
    )));
    // Keep in flight: poll nothing, render while pending.
    manager.usage.screen = Some(state);

    let backend = TestBackend::new(80, 25);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| super::render_detail(f, f.area(), &manager))
        .unwrap();

    let text = backend_text(&terminal);
    assert!(text.contains("Freshness stale · updated"));
    assert!(text.contains("Refreshing usage…"));
}

#[test]
fn render_unknown_window_shows_value_without_fabricated_bar() {
    use crate::tui::state::ManagerState;
    use ratatui::{Terminal, backend::TestBackend};

    let config = jackin_config::AppConfig::default();
    let cwd = std::path::Path::new("/test");
    let mut manager = ManagerState::from_config(&config, cwd);
    let mut account = test_account("openai", "work-id", "work");
    account.provider = "OpenAI".to_owned();
    account.windows = vec![UsageWindow {
        window_id: "mystery".to_owned(),
        rank: 0,
        category: UsageWindowCategoryV1::Other,
        label: "mystery window".to_owned(),
        value: "provider did not report".to_owned(),
        reset: String::new(),
        remaining_percent: None,
        remaining_raw_percent: None,
        used_percent: None,
        used_raw_percent: None,
        reset_at_epoch: None,
        quota_state: UsageQuotaStateV1::Unknown,
        pace_label: None,
        runs_out_label: None,
    }];
    manager.usage.screen = Some(UsageScreenState {
        accounts: vec![account],
        selected: 1,
        selected_id: Some("openai:work-id".to_owned()),
        ..UsageScreenState::default()
    });

    let backend = TestBackend::new(80, 25);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| super::render_detail(f, f.area(), &manager))
        .unwrap();

    let text = backend_text(&terminal);
    assert!(text.contains("mystery window"));
    assert!(text.contains("provider did not report"));
    assert!(
        !text.contains('█') && !text.contains('░'),
        "unknown quota must not render a bar:\n{text}"
    );
}

fn metric_group_fixture(
    group_id: &str,
    rank: u32,
    kind: jackin_protocol::usage_broker::UsageMetricGroupKindV1,
    label: &str,
    value: jackin_protocol::usage_broker::UsageMetricValueV1,
    now: i64,
) -> jackin_protocol::usage_broker::UsageMetricGroupV1 {
    use jackin_protocol::usage_broker::{
        UsageFreshnessPhaseV1, UsageMetricScopeV1, UsageQuotaStateV1,
    };
    jackin_protocol::usage_broker::UsageMetricGroupV1 {
        group_id: group_id.to_owned(),
        rank,
        kind,
        label: label.to_owned(),
        scope: UsageMetricScopeV1::default(),
        observed_at_epoch: None,
        fetched_at_epoch: now - 10,
        last_success_at_epoch: None,
        phase: UsageFreshnessPhaseV1::Current,
        is_stale: false,
        quota_state: UsageQuotaStateV1::Available,
        value,
        reset_at_epoch: None,
        renews_at_epoch: None,
        issues: Vec::new(),
    }
}

struct MetricGroupEpochs {
    retry_at: i64,
    credential_expires_at: i64,
    reset_at: i64,
    renews_at: i64,
}

/// Canonical projection carrying `Balance` + `SpendCap` + `Plan` groups, with the
/// wall-clock-relative epochs it was built against. Epochs carry bucket
/// margins: render passes real time, so asserted buckets survive a few
/// seconds of test/render skew.
fn metric_group_projection_fixture() -> (
    jackin_protocol::usage_broker::UsageProjectionV1,
    MetricGroupEpochs,
) {
    use jackin_protocol::control::Money;
    use jackin_protocol::usage_broker::{
        UsageAccountV1, UsageFreshnessPhaseV1, UsageFreshnessV1, UsageIdentityKindV1,
        UsageIssueRecoverabilityV1, UsageIssueScopeV1, UsageIssueV1, UsageLifecycleV1,
        UsageLimitWindowV1, UsageMembershipStateV1, UsageMetricGroupKindV1, UsageMetricValueV1,
        UsagePercent, UsageProjectionRefreshStateV1, UsageProjectionSchemaV1, UsageProjectionV1,
        UsageProviderV1, UsageQuotaStateV1, UsageWindowCategoryV1,
    };

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("wall clock reads")
        .as_secs() as i64;
    let epochs = MetricGroupEpochs {
        retry_at: now + 150,
        credential_expires_at: now + 30 * 86_400 + 3_600,
        reset_at: now + 90_000,
        renews_at: now + 5 * 86_400 + 3_600,
    };

    let issue = |code: &str, scope, message: &str| UsageIssueV1 {
        code: code.to_owned(),
        scope,
        recoverability: UsageIssueRecoverabilityV1::Retryable,
        message: message.to_owned(),
        retry_at_epoch: None,
    };
    let freshness = || UsageFreshnessV1 {
        generation: 1,
        phase: UsageFreshnessPhaseV1::Current,
        last_good_at_epoch: Some(now - 300),
        retry_at_epoch: None,
        is_stale: false,
    };

    let mut balance = metric_group_fixture(
        "balance",
        0,
        UsageMetricGroupKindV1::Balance,
        "Balance",
        UsageMetricValueV1::Balance {
            amount: Money::new(4_250, "USD", 2),
            expires_at_epoch: None,
        },
        now,
    );
    balance.scope.model = Some("gpt-5".to_owned());
    balance.last_success_at_epoch = Some(now - 300);
    balance.issues = vec![issue(
        "bal_delay",
        UsageIssueScopeV1::Group,
        "balance delayed",
    )];
    let mut spend = metric_group_fixture(
        "spend",
        1,
        UsageMetricGroupKindV1::SpendCap,
        "Spend cap",
        UsageMetricValueV1::SpendCap {
            cap: Some(Money::new(30_000, "USD", 2)),
            spent: Some(Money::new(5_331, "USD", 2)),
            remaining: None,
        },
        now,
    );
    spend.reset_at_epoch = Some(epochs.reset_at);
    let mut plan = metric_group_fixture(
        "plan",
        2,
        UsageMetricGroupKindV1::Plan,
        "Plan",
        UsageMetricValueV1::Plan {
            plan_label: Some("Pro".to_owned()),
            tier: None,
        },
        now,
    );
    plan.quota_state = UsageQuotaStateV1::NotApplicable;
    plan.renews_at_epoch = Some(epochs.renews_at);

    let projection = UsageProjectionV1 {
        schema_version: UsageProjectionSchemaV1,
        projection_id: "p".to_owned(),
        generated_at_epoch: now,
        discovery_revision: "d".to_owned(),
        broker_instance_id: "b".to_owned(),
        broker_generation: 1,
        refresh_state: UsageProjectionRefreshStateV1::Idle,
        providers: vec![UsageProviderV1 {
            provider_id: "openai".to_owned(),
            display_name: "OpenAI".to_owned(),
            rank: 0,
            membership_state: UsageMembershipStateV1::Current,
            freshness: freshness(),
            accounts: vec![UsageAccountV1 {
                canonical_account_id: "canon-1".to_owned(),
                identity_kind: UsageIdentityKindV1::ProviderAccountId,
                rank: 0,
                display_label: "work".to_owned(),
                plan_label: Some("Scale".to_owned()),
                status_label: None,
                lifecycle: UsageLifecycleV1::Available,
                freshness: UsageFreshnessV1 {
                    retry_at_epoch: Some(epochs.retry_at),
                    ..freshness()
                },
                provenance_count: 1,
                windows: vec![UsageLimitWindowV1 {
                    window_id: "weekly".to_owned(),
                    rank: 0,
                    category: UsageWindowCategoryV1::LongRange,
                    label: "weekly".to_owned(),
                    value_label: "120% used".to_owned(),
                    reset_label: "resets at midnight".to_owned(),
                    remaining_percent: None,
                    remaining_raw_percent: None,
                    used_percent: Some(UsagePercent::new(100).expect("valid percent")),
                    used_raw_percent: Some(120),
                    reset_at_epoch: None,
                    quota_state: UsageQuotaStateV1::Warning,
                    pace_label: Some("on pace".to_owned()),
                    runs_out_label: Some("runs out Friday".to_owned()),
                }],
                metric_groups: vec![balance, spend, plan],
                credential_expires_at_epoch: Some(epochs.credential_expires_at),
                issues: vec![issue(
                    "quota_degraded",
                    UsageIssueScopeV1::Account,
                    "quota degraded",
                )],
            }],
            issues: vec![issue(
                "prov_maint",
                UsageIssueScopeV1::Provider,
                "provider maintenance",
            )],
        }],
        unresolved: Vec::new(),
        issues: vec![issue(
            "proj_wide",
            UsageIssueScopeV1::Projection,
            "projection wide fault",
        )],
    };

    (projection, epochs)
}

#[test]
fn projection_round_trips_balance_spendcap_plan_groups_without_invented_values() {
    use jackin_protocol::control::Money;
    use jackin_protocol::usage_broker::{UsageMetricGroupKindV1, UsageMetricValueV1};

    let (projection, epochs) = metric_group_projection_fixture();

    // Projection -> state: every canonical field survives intact.
    let state = UsageScreenState::from_projection(&projection);
    assert_eq!(state.accounts.len(), 1);
    let account = &state.accounts[0];
    assert_eq!(account.plan_label, Some("Scale".to_owned()));
    assert_eq!(
        account.identity_kind,
        Some(UsageIdentityKindV1::ProviderAccountId)
    );
    assert_eq!(
        account.credential_expires_at_epoch,
        Some(epochs.credential_expires_at)
    );
    assert_eq!(account.retry_at_epoch, Some(epochs.retry_at));
    assert_eq!(account.issues.len(), 1);
    assert_eq!(account.issues[0].code, "quota_degraded");
    assert_eq!(account.provider_issues.len(), 1);
    assert_eq!(account.provider_issues[0].code, "prov_maint");
    assert_eq!(account.issue_count(), 3);
    assert_eq!(state.projection_issues.len(), 1);

    let window = &account.windows[0];
    assert_eq!(window.used_percent, Some(100));
    assert_eq!(window.used_raw_percent, Some(120));
    assert_eq!(window.quota_state, UsageQuotaStateV1::Warning);
    assert_eq!(window.pace_label, Some("on pace".to_owned()));
    assert_eq!(window.runs_out_label, Some("runs out Friday".to_owned()));

    assert_eq!(account.metric_groups.len(), 3);
    let (balance, spend, plan) = (
        &account.metric_groups[0],
        &account.metric_groups[1],
        &account.metric_groups[2],
    );
    assert_eq!(balance.kind, UsageMetricGroupKindV1::Balance);
    assert_eq!(
        balance.value,
        UsageMetricValueV1::Balance {
            amount: Money::new(4_250, "USD", 2),
            expires_at_epoch: None,
        }
    );
    assert_eq!(balance.issues.len(), 1);
    assert_eq!(balance.meter_percent(), None);
    assert_eq!(spend.kind, UsageMetricGroupKindV1::SpendCap);
    assert_eq!(
        spend.value,
        UsageMetricValueV1::SpendCap {
            cap: Some(Money::new(30_000, "USD", 2)),
            spent: Some(Money::new(5_331, "USD", 2)),
            remaining: None,
        }
    );
    assert_eq!(spend.reset_at_epoch, Some(epochs.reset_at));
    assert_eq!(spend.renews_at_epoch, None);
    assert_eq!(spend.meter_percent(), None);
    assert_eq!(plan.kind, UsageMetricGroupKindV1::Plan);
    assert_eq!(plan.reset_at_epoch, None);
    assert_eq!(plan.renews_at_epoch, Some(epochs.renews_at));
    assert_eq!(plan.meter_percent(), None);

    // State -> rendered rows, detail view.
    use crate::tui::state::ManagerState;
    use ratatui::{Terminal, backend::TestBackend};

    let config = jackin_config::AppConfig::default();
    let cwd = std::path::Path::new("/test");
    let mut manager = ManagerState::from_config(&config, cwd);
    manager.usage.screen = Some(UsageScreenState {
        accounts: state.accounts.clone(),
        selected: 1,
        selected_id: Some("openai:canon-1".to_owned()),
        detail: true,
        ..UsageScreenState::default()
    });
    let backend = TestBackend::new(120, 60);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| super::render_detail(f, f.area(), &manager))
        .unwrap();
    let detail = backend_text(&terminal);
    assert_detail_group_rows(&detail);

    // State -> rendered rows, overview panel.
    manager.usage.screen = Some(UsageScreenState {
        accounts: state.accounts.clone(),
        projection_issues: state.projection_issues.clone(),
        selected: 0,
        ..UsageScreenState::default()
    });
    let backend = TestBackend::new(120, 60);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| super::render_detail(f, f.area(), &manager))
        .unwrap();
    let overview = backend_text(&terminal);
    assert_overview_group_rows(&overview);
    assert!(
        !overview.contains("remaining"),
        "unreported remaining must not render:\n{overview}"
    );
}

fn expect_rows(haystack: &str, context: &str, expected: &[&str]) {
    for row in expected {
        assert!(
            haystack.contains(row),
            "{context} missing {row:?}:\n{haystack}"
        );
    }
}

fn assert_detail_group_rows(detail: &str) {
    expect_rows(
        detail,
        "detail",
        &[
            "Plan      Scale",
            "Identity  provider account id",
            "Credential expires in 30d",
            "Freshness updated 5m ago · retry in 2m",
            "Metric groups",
            "Balance (balance · available · updated 5m ago)",
            "scope: model gpt-5",
            "$42.50",
            "balance delayed (bal_delay)",
            "Spend cap (spend cap · available · never updated)",
            "cap $300.00 · spent $53.31",
            "resets in 1d",
            "Plan (plan · n/a · never updated)",
            "Pro",
            "renews in 5d",
            "quota: warning",
            "raw used 120%",
            "pace: on pace",
            "runs out: runs out Friday",
            "Issues",
            "quota degraded (quota_degraded)",
            "provider: provider maintenance (prov_maint)",
        ],
    );
    // No invented values: absent money/tiers/expiry/renewal/reset render nothing.
    assert!(
        !detail.contains("remaining"),
        "unreported remaining must not render:\n{detail}"
    );
    assert!(
        !detail.contains("tier"),
        "unreported tier must not render:\n{detail}"
    );
    assert!(
        !detail.contains("uncapped"),
        "reported cap must not read uncapped:\n{detail}"
    );
    assert_eq!(
        detail.matches("expires").count(),
        1,
        "only the credential expiry may use `expires`:\n{detail}"
    );
    assert_eq!(
        detail.matches("resets in").count(),
        1,
        "reset must render exactly once, on the spend-cap group:\n{detail}"
    );
    assert_eq!(
        detail.matches("renews in").count(),
        1,
        "renewal must render exactly once, on the plan group:\n{detail}"
    );
}

fn assert_overview_group_rows(overview: &str) {
    expect_rows(
        overview,
        "overview",
        &[
            "Plan Scale",
            "Credential expires in 30d",
            "Balance: $42.50",
            "Spend cap: cap $300.00 · spent $53.31",
            "Plan: Pro",
            "resets in 1d",
            "renews in 5d",
            "[Balance] balance delayed (bal_delay)",
            "quota degraded (quota_degraded)",
            "projection wide fault (proj_wide)",
        ],
    );
}

#[test]
fn metric_group_meter_only_for_window_kind() {
    use super::UsageMetricGroup;
    use jackin_protocol::usage_broker::{
        UsageMetricGroupKindV1, UsageMetricPeriodV1, UsageMetricScopeV1, UsageMetricValueV1,
        UsagePercent,
    };

    let group = |kind, value| UsageMetricGroup {
        group_id: "g".to_owned(),
        rank: 0,
        kind,
        label: "g".to_owned(),
        scope: UsageMetricScopeV1::default(),
        observed_at_epoch: None,
        fetched_at_epoch: 0,
        last_success_at_epoch: None,
        phase: UsageFreshnessPhaseV1::Current,
        is_stale: false,
        quota_state: UsageQuotaStateV1::Available,
        value,
        reset_at_epoch: None,
        renews_at_epoch: None,
        issues: Vec::new(),
    };

    let window = group(
        UsageMetricGroupKindV1::Window,
        UsageMetricValueV1::Window {
            remaining_percent: None,
            remaining_raw_percent: None,
            used_percent: Some(UsagePercent::new(30).expect("valid percent")),
            used_raw_percent: Some(30),
            period: UsageMetricPeriodV1::Unknown,
            unit: None,
        },
    );
    assert_eq!(window.meter_percent(), Some(70));

    let unknown = group(
        UsageMetricGroupKindV1::Window,
        UsageMetricValueV1::Window {
            remaining_percent: None,
            remaining_raw_percent: None,
            used_percent: None,
            used_raw_percent: None,
            period: UsageMetricPeriodV1::Unknown,
            unit: None,
        },
    );
    assert_eq!(unknown.meter_percent(), None);
}

fn manager_with_usage(state: UsageScreenState) -> crate::tui::state::ManagerState<'static> {
    let config = jackin_config::AppConfig::default();
    let mut manager =
        crate::tui::state::ManagerState::from_config(&config, std::path::Path::new("/test"));
    manager.usage.screen = Some(state);
    manager
}

fn render_detail_text(state: UsageScreenState) -> String {
    use ratatui::{Terminal, backend::TestBackend};
    let manager = manager_with_usage(state);
    let backend = TestBackend::new(120, 60);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| super::render_detail(f, f.area(), &manager))
        .unwrap();
    backend_text(&terminal)
}

fn render_list_text(state: UsageScreenState) -> String {
    use ratatui::{Terminal, backend::TestBackend};
    let manager = manager_with_usage(state);
    let backend = TestBackend::new(120, 60);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| super::render_account_list(f, f.area(), &manager))
        .unwrap();
    backend_text(&terminal)
}

fn press_key(manager: &mut crate::tui::state::ManagerState<'_>, code: crossterm::event::KeyCode) {
    use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    super::handle_key(
        manager,
        KeyEvent {
            code,
            modifiers: KeyModifiers::empty(),
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        },
    );
}

fn usage_issue(code: &str, message: &str) -> jackin_protocol::usage_broker::UsageIssueV1 {
    use jackin_protocol::usage_broker::{
        UsageIssueRecoverabilityV1, UsageIssueScopeV1, UsageIssueV1,
    };
    UsageIssueV1 {
        code: code.to_owned(),
        scope: UsageIssueScopeV1::Account,
        recoverability: UsageIssueRecoverabilityV1::Retryable,
        message: message.to_owned(),
        retry_at_epoch: None,
    }
}

fn window_metric_group(label: &str, remaining: Option<u8>, now: i64) -> super::UsageMetricGroup {
    use jackin_protocol::usage_broker::{
        UsageMetricGroupKindV1, UsageMetricPeriodV1, UsageMetricScopeV1, UsageMetricValueV1,
        UsagePercent, UsageQuotaStateV1,
    };
    super::UsageMetricGroup {
        group_id: format!("{label}-id"),
        rank: 0,
        kind: UsageMetricGroupKindV1::Window,
        label: label.to_owned(),
        scope: UsageMetricScopeV1::default(),
        observed_at_epoch: None,
        fetched_at_epoch: now - 10,
        last_success_at_epoch: Some(now - 60),
        phase: UsageFreshnessPhaseV1::Current,
        is_stale: false,
        quota_state: UsageQuotaStateV1::Available,
        value: UsageMetricValueV1::Window {
            remaining_percent: remaining.map(|p| UsagePercent::new(p).expect("valid percent")),
            remaining_raw_percent: remaining.map(i32::from),
            used_percent: None,
            used_raw_percent: None,
            period: UsageMetricPeriodV1::Unknown,
            unit: None,
        },
        reset_at_epoch: None,
        renews_at_epoch: None,
        issues: Vec::new(),
    }
}

#[test]
fn detail_toggle_renders_summary_vs_full_bodies() {
    let mut account = test_account("openai", "a", "work");
    account.windows = vec![UsageWindow {
        quota_state: UsageQuotaStateV1::Warning,
        pace_label: Some("on pace".to_owned()),
        runs_out_label: Some("runs out Friday".to_owned()),
        ..test_window("weekly", Some(73))
    }];
    account.metric_groups = vec![window_metric_group("Session", Some(5), 1_800_000_000)];
    account.issues = vec![usage_issue("quota_degraded", "quota degraded")];

    let summary_state = UsageScreenState {
        accounts: vec![account.clone()],
        selected: 1,
        selected_id: Some("openai:a".to_owned()),
        detail: false,
        ..UsageScreenState::default()
    };
    let summary = render_detail_text(summary_state);
    assert!(
        summary.contains("weekly"),
        "summary keeps labels:\n{summary}"
    );
    assert!(
        summary.contains("weekly value"),
        "summary keeps values:\n{summary}"
    );
    assert!(
        summary.contains("Session: 5% remaining"),
        "summary keeps compact group rows:\n{summary}"
    );
    assert!(
        summary.contains("1 issue · Enter for detail"),
        "summary collapses issues to a count:\n{summary}"
    );
    assert!(summary.contains("Enter for full detail"));
    for full_only in [
        "quota: warning",
        "pace: on pace",
        "runs out: runs out Friday",
        "quota degraded",
        "(window ·",
        "Enter for summary",
    ] {
        assert!(
            !summary.contains(full_only),
            "summary must not render full-only {full_only:?}:\n{summary}"
        );
    }

    let full_state = UsageScreenState {
        accounts: vec![account],
        selected: 1,
        selected_id: Some("openai:a".to_owned()),
        detail: true,
        ..UsageScreenState::default()
    };
    let full = render_detail_text(full_state);
    for row in [
        "quota: warning",
        "pace: on pace",
        "runs out: runs out Friday",
        "quota degraded (quota_degraded)",
        "Session (window · available · ",
        "Enter for summary",
    ] {
        assert!(full.contains(row), "full view missing {row:?}:\n{full}");
    }
    assert!(
        !full.contains("Enter for full detail"),
        "full view must not carry the summary hint:\n{full}"
    );
}

#[test]
fn sort_orders_by_remaining_then_unknown_and_by_name() {
    use super::{UsageFilter, UsageSort};

    let mut low = test_account("openai", "a", "zebra");
    low.windows = vec![test_window("w", Some(12))];
    let mut high = test_account("anthropic", "b", "mike");
    high.windows = vec![test_window("w", Some(90))];
    let mut unknown = test_account("zai", "c", "alpha");
    unknown.windows = vec![test_window("w", None)];
    let mut state = UsageScreenState {
        accounts: vec![high, unknown, low],
        ..UsageScreenState::default()
    };

    assert_eq!(state.sort, UsageSort::Provider);
    assert_eq!(state.filter, UsageFilter::All);
    assert_eq!(state.visible_order(), vec![0, 1, 2]);

    state.sort = UsageSort::Remaining;
    assert_eq!(state.visible_order(), vec![2, 0, 1]);

    state.sort = UsageSort::Name;
    assert_eq!(state.visible_order(), vec![1, 0, 2]);

    assert_eq!(UsageSort::Provider.cycle(), UsageSort::Remaining);
    assert_eq!(UsageSort::Remaining.cycle(), UsageSort::Name);
    assert_eq!(UsageSort::Name.cycle(), UsageSort::Provider);
}

#[test]
fn filter_predicate_matches_issues_and_stale_membership() {
    use super::UsageFilter;

    let mut bad = test_account("openai", "a", "bad");
    bad.issues = vec![usage_issue("boom", "boom")];
    let mut stale = test_account("anthropic", "b", "stale");
    stale.is_stale = true;
    let ok = test_account("zai", "c", "ok");
    let mut state = UsageScreenState {
        accounts: vec![bad, stale, ok],
        ..UsageScreenState::default()
    };

    state.filter = UsageFilter::Issues;
    assert_eq!(state.visible_order(), vec![0]);
    state.filter = UsageFilter::Stale;
    assert_eq!(state.visible_order(), vec![1]);
    state.filter = UsageFilter::All;
    assert_eq!(state.visible_order(), vec![0, 1, 2]);

    assert_eq!(UsageFilter::All.cycle(), UsageFilter::Issues);
    assert_eq!(UsageFilter::Issues.cycle(), UsageFilter::Stale);
    assert_eq!(UsageFilter::Stale.cycle(), UsageFilter::All);
}

#[test]
fn sort_filter_keys_cycle_reanchor_and_render_state() {
    use super::{UsageFilter, UsageSort};
    use crossterm::event::KeyCode;

    let mut low = test_account("openai", "a", "zebra");
    low.windows = vec![test_window("w", Some(12))];
    let high = test_account("anthropic", "b", "mike");
    let mut manager = manager_with_usage(UsageScreenState {
        accounts: vec![high, low],
        selected: 2,
        selected_id: Some("openai:a".to_owned()),
        ..UsageScreenState::default()
    });

    press_key(&mut manager, KeyCode::Char('s'));
    let screen = manager.usage.screen.as_ref().unwrap();
    assert_eq!(screen.sort, UsageSort::Remaining);
    // Selection follows the account by stable id: lowest remaining first.
    assert_eq!(screen.selected, 1);
    assert_eq!(screen.selected_id, Some("openai:a".to_owned()));

    press_key(&mut manager, KeyCode::Char('f'));
    let screen = manager.usage.screen.as_ref().unwrap();
    assert_eq!(screen.filter, UsageFilter::Issues);
    // Neither account has issues: selection parks on Overview, id kept.
    assert_eq!(screen.selected, 0);
    assert_eq!(screen.selected_id, Some("openai:a".to_owned()));

    let list = render_list_text(manager.usage.screen.clone().unwrap());
    assert!(list.contains("s:sort(remaining) f:filter(issues) c:constrained"));
    assert!(list.contains("No accounts match filter 'issues'."));
    assert!(list.contains("Press f to cycle the filter."));
}

#[test]
fn min_remaining_spans_windows_and_groups_with_unknown_last() {
    let mut mixed = test_account("openai", "a", "mixed");
    mixed.windows = vec![test_window("w", None)];
    mixed.metric_groups = vec![window_metric_group("Session", Some(5), 1_800_000_000)];
    assert_eq!(mixed.min_remaining(), Some(5));

    let mut unknown = test_account("zai", "b", "unknown");
    unknown.windows = vec![test_window("w", None)];
    assert_eq!(unknown.min_remaining(), None);
}

#[test]
fn capacity_finder_selects_most_constrained_with_reset_tiebreak() {
    let mut far = test_account("openai", "a", "far");
    far.windows = vec![UsageWindow {
        reset_at_epoch: Some(1_900_000_000),
        ..test_window("w", Some(20))
    }];
    let mut soon = test_account("anthropic", "b", "soon");
    soon.windows = vec![UsageWindow {
        reset_at_epoch: Some(1_800_000_100),
        ..test_window("w", Some(20))
    }];
    let mut roomy = test_account("zai", "c", "roomy");
    roomy.windows = vec![test_window("w", Some(80))];
    let mut state = UsageScreenState {
        accounts: vec![far, soon, roomy],
        ..UsageScreenState::default()
    };

    assert_eq!(state.most_constrained_selected(), Some(2));
    assert!(state.jump_to_most_constrained());
    assert_eq!(state.selected, 2);
    assert_eq!(state.selected_id, Some("anthropic:b".to_owned()));

    // An exhausted account beats every partial one.
    state.accounts[2].windows = vec![test_window("w", Some(0))];
    assert_eq!(state.most_constrained_selected(), Some(3));
}

#[test]
fn capacity_finder_empty_states_post_notice_without_moving() {
    use super::UsageFilter;

    let mut empty = UsageScreenState::open_with_snapshot(Vec::new(), None);
    assert_eq!(empty.most_constrained_selected(), None);
    assert!(!empty.jump_to_most_constrained());
    assert_eq!(empty.selected, 0);
    assert_eq!(
        empty.notice,
        Some("No usage accounts configured; nothing to compare".to_owned())
    );

    let mut filtered = UsageScreenState {
        accounts: vec![test_account("openai", "a", "work")],
        filter: UsageFilter::Issues,
        ..UsageScreenState::default()
    };
    assert!(!filtered.jump_to_most_constrained());
    assert_eq!(filtered.selected, 0);
    assert_eq!(
        filtered.notice,
        Some("No accounts match filter 'issues'; press f to clear".to_owned())
    );

    let mut unknown = test_account("openai", "a", "work");
    unknown.windows = vec![test_window("w", None)];
    let mut no_percent = UsageScreenState {
        accounts: vec![unknown],
        ..UsageScreenState::default()
    };
    assert!(!no_percent.jump_to_most_constrained());
    assert_eq!(
        no_percent.notice,
        Some("No visible account reports remaining quota".to_owned())
    );
}

#[test]
fn capacity_finder_key_jumps_selection() {
    use crossterm::event::KeyCode;

    let mut low = test_account("openai", "a", "zebra");
    low.windows = vec![test_window("w", Some(12))];
    let high = test_account("anthropic", "b", "mike");
    let mut manager = manager_with_usage(UsageScreenState {
        accounts: vec![high, low],
        ..UsageScreenState::default()
    });
    press_key(&mut manager, KeyCode::Char('c'));
    let screen = manager.usage.screen.as_ref().unwrap();
    assert_eq!(screen.selected, 2);
    assert_eq!(screen.selected_id, Some("openai:a".to_owned()));
}

#[test]
fn labels_align_with_capsule_tab_vocabulary() {
    use super::{lifecycle_label, quota_state_label, well_known_provider_name};

    // Lifecycle words shared with Capsule `usage_tab_status_label` match
    // exactly; `available`/`not started` are console-owned (Capsule's healthy
    // word `fresh` belongs to the freshness axis, a different concept).
    for (lifecycle, expected) in [
        (UsageLifecycleV1::Available, "available"),
        (UsageLifecycleV1::AgentUninitialized, "not started"),
        (UsageLifecycleV1::NeedsLogin, "needs login"),
        (UsageLifecycleV1::NeedsSecret, "needs secret"),
        (UsageLifecycleV1::Unsupported, "unsupported"),
        (UsageLifecycleV1::Unavailable, "unavailable"),
        (UsageLifecycleV1::Error, "error"),
    ] {
        assert_eq!(lifecycle_label(lifecycle), expected);
    }

    // Capsule tabs carry no quota axis, so the full quota table is pinned
    // here to catch drift against the console's own render contract.
    for (state, expected) in [
        (UsageQuotaStateV1::Available, "available"),
        (UsageQuotaStateV1::NotStarted, "not started"),
        (UsageQuotaStateV1::Warning, "warning"),
        (UsageQuotaStateV1::Exhausted, "exhausted"),
        (UsageQuotaStateV1::Unsupported, "unsupported"),
        (UsageQuotaStateV1::Unavailable, "unavailable"),
        (UsageQuotaStateV1::NoPermission, "no permission"),
        (UsageQuotaStateV1::Unknown, "unknown"),
        (UsageQuotaStateV1::NotApplicable, "n/a"),
        (UsageQuotaStateV1::Error, "error"),
    ] {
        assert_eq!(quota_state_label(state), expected);
    }

    // Provider display names mirror Capsule `provider_display_label`.
    for (provider_id, expected) in [
        ("anthropic", "Anthropic"),
        ("claude", "Anthropic"),
        ("openai", "OpenAI"),
        ("codex", "OpenAI"),
        ("opencode", "OpenCode"),
        ("kimi", "Kimi"),
        ("moonshot", "Kimi"),
        ("grok", "xAI"),
        ("xai", "xAI"),
        ("amp", "Amp"),
        ("zai", "Z.AI"),
        ("minimax", "MiniMax"),
        ("acme", "acme"),
    ] {
        assert_eq!(well_known_provider_name(provider_id), expected);
    }

    // Freshness ages: sub-minute matches Capsule `relative_updated_label`
    // modulo the console's lowercase row style; stale keeps the tab `stale`
    // prefix with the same ` · ` separator.
    let now = 1_800_000_000;
    let mut account = test_account("openai", "a", "work");
    account.freshness_phase = UsageFreshnessPhaseV1::Refreshing;
    assert_eq!(freshness_age_label(now, &account), "refreshing…");
    account.freshness_phase = UsageFreshnessPhaseV1::Current;
    account.last_good_at_epoch = None;
    assert_eq!(freshness_age_label(now, &account), "never updated");
    account.last_good_at_epoch = Some(now - 10);
    assert_eq!(freshness_age_label(now, &account), "updated now");
    account.last_good_at_epoch = Some(now - 300);
    assert_eq!(freshness_age_label(now, &account), "updated 5m ago");
    account.is_stale = true;
    assert_eq!(freshness_age_label(now, &account), "stale · updated 5m ago");
}

fn backend_text(terminal: &ratatui::Terminal<ratatui::backend::TestBackend>) -> String {
    let buffer = terminal.backend().buffer().clone();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
