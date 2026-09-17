// SPDX-FileCopyrightText: 2026 Alexey Zhokov
// SPDX-License-Identifier: Apache-2.0

use std::time::{Duration, Instant};

use super::{
    USAGE_HEARTBEAT_INTERVAL, UsageAccount, UsageScreenState, UsageWindow, freshness_age_label,
    meter_line,
};
use jackin_protocol::usage_broker::{UsageFreshnessPhaseV1, UsageLifecycleV1};

fn test_window(label: &str, remaining: Option<u8>) -> UsageWindow {
    UsageWindow {
        window_id: format!("{label}-id"),
        label: label.to_owned(),
        value: format!("{label} value"),
        reset: "resets soon".to_owned(),
        remaining_percent: remaining,
        used_percent: None,
        reset_at_epoch: Some(1_800_000_000),
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
        is_stale: false,
        windows: vec![test_window("weekly", Some(73))],
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
    assert_eq!(freshness_age_label(now, &account), "updated just now");
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

    let mut openai = test_account("openai", "work-id", "work");
    openai.provider = "OpenAI".to_owned();
    openai.windows = vec![
        UsageWindow {
            window_id: "session".to_owned(),
            label: "5h session".to_owned(),
            value: "10% left".to_owned(),
            reset: "resets in 2h".to_owned(),
            remaining_percent: Some(10),
            used_percent: None,
            reset_at_epoch: None,
        },
        UsageWindow {
            window_id: "weekly".to_owned(),
            label: "weekly".to_owned(),
            value: "80% left".to_owned(),
            reset: "resets in 5d".to_owned(),
            remaining_percent: Some(80),
            used_percent: None,
            reset_at_epoch: None,
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
        label: "mystery window".to_owned(),
        value: "provider did not report".to_owned(),
        reset: String::new(),
        remaining_percent: None,
        used_percent: None,
        reset_at_epoch: None,
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
