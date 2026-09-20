// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use jackin_protocol::usage_broker::{
    UsageAccountV1, UsageFreshnessPhaseV1, UsageFreshnessV1, UsageIdentityKindV1,
    UsageIssueRecoverabilityV1, UsageIssueScopeV1, UsageIssueV1, UsageLifecycleV1,
    UsageLimitWindowV1, UsageMembershipStateV1, UsagePercent, UsageProjectionRefreshStateV1,
    UsageProjectionSchemaV1, UsageProjectionV1, UsageProviderV1, UsageQuotaStateV1,
    UsageUnresolvedV1, UsageWindowCategoryV1,
};

use super::{
    UsageEntryId, UsageFocus, UsageScreenState, USAGE_HEARTBEAT_INTERVAL, entry_display_label,
    freshness_age_label, meter_line, window_meter_percent,
};

fn quota_window(
    window_id: &str,
    label: &str,
    category: UsageWindowCategoryV1,
    remaining: Option<u8>,
    used: Option<u8>,
) -> UsageLimitWindowV1 {
    UsageLimitWindowV1 {
        window_id: window_id.to_owned(),
        rank: 0,
        category,
        label: label.to_owned(),
        value_label: remaining.map_or_else(
            || used.map_or_else(|| "provider did not report".to_owned(), |n| format!("{n}% used")),
            |n| format!("{n}% left"),
        ),
        reset_label: "resets soon".to_owned(),
        remaining_percent: remaining.map(|n| UsagePercent::new(n).expect("valid percent")),
        remaining_raw_percent: remaining.map(i32::from),
        used_percent: used.map(|n| UsagePercent::new(n).expect("valid percent")),
        used_raw_percent: used.map(i32::from),
        reset_at_epoch: Some(1_800_000_000),
        quota_state: UsageQuotaStateV1::Available,
        pace_label: None,
        runs_out_label: None,
    }
}

fn freshness(
    phase: UsageFreshnessPhaseV1,
    last_good_at_epoch: Option<i64>,
    is_stale: bool,
) -> UsageFreshnessV1 {
    UsageFreshnessV1 {
        generation: 1,
        phase,
        last_good_at_epoch,
        retry_at_epoch: None,
        is_stale,
    }
}

fn account(
    canonical_account_id: &str,
    display_label: &str,
    windows: Vec<UsageLimitWindowV1>,
) -> UsageAccountV1 {
    UsageAccountV1 {
        canonical_account_id: canonical_account_id.to_owned(),
        identity_kind: UsageIdentityKindV1::ProviderStableHandle,
        rank: 0,
        display_label: display_label.to_owned(),
        plan_label: None,
        status_label: None,
        lifecycle: UsageLifecycleV1::Available,
        freshness: freshness(
            UsageFreshnessPhaseV1::Current,
            Some(1_799_999_000),
            false,
        ),
        provenance_count: 1,
        windows,
        metric_groups: Vec::new(),
        credential_expires_at_epoch: None,
        issues: Vec::new(),
    }
}

fn provider(
    provider_id: &str,
    display_name: &str,
    rank: u32,
    accounts: Vec<UsageAccountV1>,
) -> UsageProviderV1 {
    UsageProviderV1 {
        provider_id: provider_id.to_owned(),
        display_name: display_name.to_owned(),
        rank,
        membership_state: UsageMembershipStateV1::Current,
        freshness: freshness(UsageFreshnessPhaseV1::Current, None, false),
        accounts,
        issues: Vec::new(),
    }
}

fn unresolved(provider_id: &str, capability_id: &str) -> UsageUnresolvedV1 {
    UsageUnresolvedV1 {
        provider_id: provider_id.to_owned(),
        capability_id: capability_id.to_owned(),
        configuration_count: 1,
        state: UsageLifecycleV1::NeedsLogin,
        issues: vec![UsageIssueV1 {
            code: "auth_required".to_owned(),
            scope: UsageIssueScopeV1::Account,
            recoverability: UsageIssueRecoverabilityV1::ActionRequired,
            message: "authentication required".to_owned(),
            retry_at_epoch: None,
        }],
    }
}

fn projection(
    providers: Vec<UsageProviderV1>,
    unresolved: Vec<UsageUnresolvedV1>,
) -> UsageProjectionV1 {
    UsageProjectionV1 {
        schema_version: UsageProjectionSchemaV1,
        projection_id: "fixture-projection".to_owned(),
        generated_at_epoch: 1_800_000_000,
        discovery_revision: "fixture-discovery".to_owned(),
        broker_instance_id: "fixture-broker".to_owned(),
        broker_generation: 1,
        refresh_state: UsageProjectionRefreshStateV1::Idle,
        providers,
        unresolved,
        issues: Vec::new(),
    }
}

fn empty_projection() -> UsageProjectionV1 {
    projection(Vec::new(), Vec::new())
}

fn single_account_projection() -> UsageProjectionV1 {
    projection(
        vec![provider(
            "openai",
            "OpenAI",
            0,
            vec![account(
                "openai-work",
                "Work",
                vec![quota_window(
                    "weekly",
                    "Weekly",
                    UsageWindowCategoryV1::LongRange,
                    Some(73),
                    None,
                )],
            )],
        )],
        Vec::new(),
    )
}

fn shared_provider_projection() -> UsageProjectionV1 {
    projection(
        vec![provider(
            "anthropic",
            "Anthropic / Claude",
            0,
            vec![
                account(
                    "claude-work",
                    "Claude Work",
                    vec![
                        quota_window(
                            "session",
                            "5h session",
                            UsageWindowCategoryV1::Session,
                            Some(10),
                            None,
                        ),
                        quota_window(
                            "weekly",
                            "Weekly",
                            UsageWindowCategoryV1::LongRange,
                            Some(80),
                            None,
                        ),
                    ],
                ),
                account(
                    "claude-personal",
                    "Claude Personal",
                    vec![quota_window(
                        "weekly",
                        "Weekly",
                        UsageWindowCategoryV1::LongRange,
                        Some(50),
                        None,
                    )],
                ),
            ],
        )],
        Vec::new(),
    )
}

fn press(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: KeyEventState::empty(),
    }
}

fn render_text(
    width: u16,
    height: u16,
    projection: UsageProjectionV1,
    screen: UsageScreenState,
) -> String {
    use crate::tui::state::ManagerState;
    use ratatui::{Terminal, backend::TestBackend};

    let config = jackin_config::AppConfig::default();
    let mut manager = ManagerState::from_config(&config, std::path::Path::new("/test"));
    manager.usage_projection = Some(projection);
    manager.usage.screen = Some(screen);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| super::render(frame, frame.area(), &manager))
        .unwrap();
    backend_text(&terminal)
}

#[test]
fn meter_scales_to_remaining_and_used_percentages() {
    assert_eq!(meter_line(10, Some(50)), Some("  █████░░░░░".to_owned()));
    assert_eq!(meter_line(10, Some(0)), Some("  ░░░░░░░░░░".to_owned()));
    assert_eq!(meter_line(10, Some(100)), Some("  ██████████".to_owned()));
    assert_eq!(meter_line(10, None), None);

    let used = quota_window(
        "weekly",
        "Weekly",
        UsageWindowCategoryV1::LongRange,
        None,
        Some(30),
    );
    assert_eq!(window_meter_percent(&used), Some(70));
    let unknown = quota_window(
        "weekly",
        "Weekly",
        UsageWindowCategoryV1::LongRange,
        None,
        None,
    );
    assert_eq!(window_meter_percent(&unknown), None);
}

#[test]
fn selection_stays_in_bounds_and_keeps_two_same_provider_accounts_distinct() {
    let projection = shared_provider_projection();
    let mut state = UsageScreenState::from_projection(&projection);
    assert_eq!(state.entries().len(), 2);

    state.move_selection(1);
    assert_eq!(state.selected, 1);
    assert_eq!(
        state.selected_id,
        Some(UsageEntryId::Account {
            provider_id: "anthropic".to_owned(),
            canonical_account_id: "claude-work".to_owned(),
        })
    );
    state.move_selection(1);
    assert_eq!(state.selected, 2);
    assert_eq!(
        state.selected_id,
        Some(UsageEntryId::Account {
            provider_id: "anthropic".to_owned(),
            canonical_account_id: "claude-personal".to_owned(),
        })
    );
    state.move_selection(8);
    assert_eq!(state.selected, 2);
    state.move_selection(-9);
    assert_eq!(state.selected, 0);
    assert_eq!(state.selected_id, None);
}

#[test]
fn refresh_preserves_selection_across_account_rename_and_provider_reorder() {
    let now = Instant::now();
    let initial = projection(
        vec![
            provider(
                "openai",
                "OpenAI",
                0,
                vec![account("openai-work", "Work", Vec::new())],
            ),
            provider(
                "anthropic",
                "Anthropic",
                1,
                vec![account("claude-personal", "Personal", Vec::new())],
            ),
        ],
        Vec::new(),
    );
    let mut state = UsageScreenState::from_projection(&initial);
    state.move_selection(1);
    assert_eq!(
        state.selected_id,
        Some(UsageEntryId::Account {
            provider_id: "openai".to_owned(),
            canonical_account_id: "openai-work".to_owned(),
        })
    );

    let reordered = projection(
        vec![
            provider(
                "anthropic",
                "Anthropic",
                0,
                vec![account("claude-personal", "Personal", Vec::new())],
            ),
            provider(
                "openai",
                "OpenAI renamed",
                1,
                vec![account("openai-work", "Work renamed", Vec::new())],
            ),
        ],
        Vec::new(),
    );
    state.apply_refresh(reordered, now);
    assert_eq!(state.selected, 2);
    assert_eq!(state.selected_entry().unwrap().account().unwrap().display_label, "Work renamed");
    assert_eq!(state.selected_id, Some(UsageEntryId::Account {
        provider_id: "openai".to_owned(),
        canonical_account_id: "openai-work".to_owned(),
    }));
    assert!(state.notice.is_none());
    assert_eq!(state.last_refresh_at, Some(now));
}

#[test]
fn removed_account_falls_back_to_overview_and_keeps_notice() {
    let initial = shared_provider_projection();
    let mut state = UsageScreenState::from_projection(&initial);
    state.move_selection(2);
    state.focus = UsageFocus::Detail;
    let reduced = projection(
        vec![provider(
            "anthropic",
            "Anthropic / Claude",
            0,
            vec![account("claude-work", "Claude Work", Vec::new())],
        )],
        Vec::new(),
    );

    state.apply_refresh(reduced, Instant::now());
    assert_eq!(state.selected, 0);
    assert_eq!(state.selected_id, None);
    assert_eq!(state.notice.as_deref(), Some("Previously selected account unavailable; showing Overview"));
    assert_eq!(state.focus, UsageFocus::List);
}

#[test]
fn failed_refresh_preserves_last_good_projection_and_selection() {
    let mut state = UsageScreenState::from_projection(&single_account_projection());
    state.move_selection(1);
    let before = state.projection.clone();
    let now = Instant::now();
    state.apply_refresh_error("Usage unavailable: timeout".to_owned(), now);

    assert_eq!(state.projection, before);
    assert_eq!(state.selected, 1);
    assert_eq!(state.notice.as_deref(), Some("Usage unavailable: timeout"));
    assert_eq!(state.last_refresh_at, Some(now));
    assert!(!state.refresh_due);
}

#[test]
fn heartbeat_starts_after_first_completion_and_refresh_polling_is_generation_safe() {
    let mut state = UsageScreenState::open_with_snapshot(None, None);
    assert!(state.refresh_due);
    assert!(!state.heartbeat_due(Instant::now()));
    let open = state
        .next_refresh_plan_if_due(Instant::now())
        .expect("open requests an initial refresh");
    state.begin_refresh(crate::tui::runtime::ready_blocking_subscription((
        open.generation,
        Ok(empty_projection()),
    )));
    let result = state.poll_refresh().expect("ready result").unwrap();
    state.apply_refresh(result, Instant::now());
    assert!(!state.heartbeat_due(Instant::now()));

    let completed = Instant::now()
        .checked_sub(USAGE_HEARTBEAT_INTERVAL + Duration::from_secs(1))
        .expect("heartbeat interval fits in uptime");
    state.last_refresh_at = Some(completed);
    assert!(state.heartbeat_due(Instant::now()));

    let mut stale = UsageScreenState::open_with_snapshot(None, None);
    let plan = stale
        .next_refresh_plan_if_due(Instant::now())
        .expect("open requests an initial refresh");
    stale.begin_refresh(crate::tui::runtime::ready_blocking_subscription((
        plan.generation.wrapping_add(1),
        Ok(single_account_projection()),
    )));
    assert!(stale.poll_refresh().is_none());
    assert!(!stale.refresh_in_flight());
    assert!(stale.projection.is_none());
}

#[test]
fn refresh_joins_in_flight_work_and_force_is_manual_only() {
    let mut state = UsageScreenState::open_with_snapshot(None, None);
    let open = state
        .next_refresh_plan_if_due(Instant::now())
        .expect("open marks refresh due");
    assert!(!open.force);
    state.begin_refresh(crate::tui::runtime::ready_blocking_subscription((
        open.generation,
        Ok(empty_projection()),
    )));
    state.refresh_due = true;
    state.force_refresh_pending = true;
    assert!(state.next_refresh_plan_if_due(Instant::now()).is_none());
    assert!(!state.refresh_due);

    state.poll_refresh();
    state.apply_refresh(empty_projection(), Instant::now());
    assert!(state.next_refresh_plan_if_due(Instant::now()).is_none());
    state.refresh_due = true;
    state.force_refresh_pending = true;
    assert!(state
        .next_refresh_plan_if_due(Instant::now())
        .expect("manual refresh").force);
    assert!(!state.force_refresh_pending);
}

#[test]
fn clone_drops_in_flight_handle_but_preserves_projection_and_generation() {
    let projection = single_account_projection();
    let mut state = UsageScreenState::from_projection(&projection);
    let plan = state
        .next_refresh_plan_if_due(Instant::now())
        .expect("heartbeat/open request due");
    state.begin_refresh(crate::tui::runtime::ready_blocking_subscription((
        plan.generation,
        Ok(empty_projection()),
    )));
    let cloned = state.clone();
    assert!(state.refresh_in_flight());
    assert!(!cloned.refresh_in_flight());
    assert_eq!(cloned.projection, Some(projection));
    assert_eq!(cloned.refresh_generation, state.refresh_generation);
    assert_eq!(cloned, state);
}

#[test]
fn open_marks_refresh_due_and_manual_key_requests_forced_refresh() {
    let state = UsageScreenState::open_with_snapshot(Some(single_account_projection()), None);
    assert!(state.refresh_due);
    assert_eq!(state.selected, 0);
    assert_eq!(state.selected_id, None);

    use crate::tui::state::ManagerState;
    let config = jackin_config::AppConfig::default();
    let mut manager = ManagerState::from_config(&config, std::path::Path::new("/test"));
    manager.usage.screen = Some(UsageScreenState::open_with_snapshot(None, None));
    manager
        .usage
        .screen
        .as_mut()
        .unwrap()
        .apply_refresh(empty_projection(), Instant::now());
    super::handle_key(&mut manager, press(KeyCode::Char('r')));
    let screen = manager.usage.screen.as_ref().unwrap();
    assert!(screen.refresh_due);
    assert!(screen.force_refresh_pending);
}

#[test]
fn enter_and_escape_reverse_focus_and_restore_overview_after_removal() {
    use crate::tui::state::ManagerState;
    let config = jackin_config::AppConfig::default();
    let mut manager = ManagerState::from_config(&config, std::path::Path::new("/test"));
    manager.usage.visible = true;
    manager.usage.screen = Some(UsageScreenState::open_with_snapshot(
        Some(single_account_projection()),
        None,
    ));

    super::handle_key(&mut manager, press(KeyCode::Down));
    assert_eq!(manager.usage.screen.as_ref().unwrap().selected, 1);
    super::handle_key(&mut manager, press(KeyCode::Enter));
    assert_eq!(manager.usage.screen.as_ref().unwrap().focus, UsageFocus::Detail);
    super::handle_key(&mut manager, press(KeyCode::Esc));
    assert_eq!(manager.usage.screen.as_ref().unwrap().focus, UsageFocus::List);
    super::handle_key(&mut manager, press(KeyCode::Esc));
    assert!(!manager.usage.visible);
}

#[test]
fn freshness_age_uses_explicit_clock_and_distinguishes_stale() {
    let now = 1_800_000_000;
    assert_eq!(
        freshness_age_label(now, UsageFreshnessPhaseV1::Refreshing, None, false),
        "refreshing…"
    );
    assert_eq!(
        freshness_age_label(now, UsageFreshnessPhaseV1::Current, None, false),
        "never updated"
    );
    assert_eq!(
        freshness_age_label(now, UsageFreshnessPhaseV1::Current, Some(now - 10), false),
        "updated just now"
    );
    assert_eq!(
        freshness_age_label(now, UsageFreshnessPhaseV1::Current, Some(now - 300), false),
        "updated 5m ago"
    );
    assert_eq!(
        freshness_age_label(now, UsageFreshnessPhaseV1::Stale, Some(now - 300), false),
        "stale · updated 5m ago"
    );
}

#[test]
fn screen_state_retains_the_complete_canonical_projection() {
    let projection = shared_provider_projection();
    let state = UsageScreenState::from_projection(&projection);
    assert_eq!(state.projection.as_ref(), Some(&projection));
    assert_eq!(state.projection.as_ref().unwrap().generated_at_epoch, 1_800_000_000);
    assert_eq!(state.entries().len(), 2);
    assert_eq!(state.entries()[0].account().unwrap().windows.len(), 2);
    assert_eq!(state.entries()[1].account().unwrap().windows[0].value_label, "50% left");
}

#[test]
fn unresolved_rows_are_grouped_by_provider_without_displaying_capability_ids() {
    let projection = projection(
        vec![provider(
            "openai",
            "OpenAI",
            0,
            vec![account("openai-work", "Work", Vec::new())],
        )],
        vec![unresolved("openai", "openai:opaque-secret-reference")],
    );
    let state = UsageScreenState::from_projection(&projection);
    let entries = state.entries();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[1].provider_label(), "OpenAI");
    assert_eq!(entry_display_label(&entries, 1), "Unresolved account 1");
    assert_eq!(super::entry_status_label(entries[1]), "needs login · authentication required");
    assert!(!entry_display_label(&entries, 1).contains("opaque-secret-reference"));
}

#[test]
fn duplicate_account_labels_get_safe_suffixes_without_merging_ids() {
    let projection = projection(
        vec![provider(
            "anthropic",
            "Anthropic / Claude",
            0,
            vec![
                account("work-one", "Work", Vec::new()),
                account("work-two", "Work", Vec::new()),
            ],
        )],
        Vec::new(),
    );
    let state = UsageScreenState::from_projection(&projection);
    let entries = state.entries();
    assert_eq!(entry_display_label(&entries, 0), "Work (1)");
    assert_eq!(entry_display_label(&entries, 1), "Work (2)");
    assert_ne!(entries[0].id(), entries[1].id());
}

#[test]
fn overview_renders_every_same_provider_account_and_all_principal_windows() {
    let text = render_text(
        120,
        30,
        shared_provider_projection(),
        UsageScreenState::from_projection(&shared_provider_projection()),
    );
    assert!(text.contains("Claude Work"));
    assert!(text.contains("Claude Personal"));
    assert!(text.contains("5h session"));
    assert!(text.contains("80% left"));
    assert!(text.contains("50% left"));
}

#[test]
fn narrow_list_and_detail_render_and_clip_long_unicode_labels_safely() {
    let mut long = account("long-account", "東京🙂é-very-long-account-label", Vec::new());
    long.windows.push(quota_window(
        "weekly",
        "Weekly",
        UsageWindowCategoryV1::LongRange,
        Some(40),
        None,
    ));
    let projection = projection(
        vec![provider("openai", "OpenAI", 0, vec![long])],
        Vec::new(),
    );
    let list = UsageScreenState::from_projection(&projection);
    let list_text = render_text(40, 16, projection.clone(), list);
    assert!(list_text.contains("Overview"));
    assert!(list_text.lines().all(|line| line.chars().count() <= 40));

    let mut detail = UsageScreenState::from_projection(&projection);
    detail.move_selection(1);
    detail.focus = UsageFocus::Detail;
    let detail_text = render_text(40, 16, projection, detail);
    assert!(detail_text.contains("Provider"));
    assert!(detail_text.contains("Weekly"));
    assert!(detail_text.lines().all(|line| line.chars().count() <= 40));
}

#[test]
fn account_detail_keeps_freshness_and_refreshing_state_visible() {
    let mut projection = single_account_projection();
    let account = &mut projection.providers[0].accounts[0];
    account.freshness = freshness(UsageFreshnessPhaseV1::Stale, Some(1_799_000_000), true);
    let mut state = UsageScreenState::from_projection(&projection);
    state.move_selection(1);
    state.focus = UsageFocus::Detail;
    let plan = state.next_refresh_plan_if_due(Instant::now());
    let generation = plan.map_or(state.refresh_generation, |plan| plan.generation);
    state.begin_refresh(crate::tui::runtime::ready_blocking_subscription((
        generation,
        Ok(empty_projection()),
    )));
    let text = render_text(120, 25, projection, state);
    assert!(text.contains("Freshness stale · updated"));
    assert!(text.contains("Refreshing usage…"));
}

#[test]
fn unknown_window_displays_value_without_fabricating_meter() {
    let projection = projection(
        vec![provider(
            "openai",
            "OpenAI",
            0,
            vec![account(
                "unknown",
                "Unknown account",
                vec![quota_window(
                    "unknown",
                    "Unknown window",
                    UsageWindowCategoryV1::Other,
                    None,
                    None,
                )],
            )],
        )],
        Vec::new(),
    );
    let mut state = UsageScreenState::from_projection(&projection);
    state.move_selection(1);
    state.focus = UsageFocus::Detail;
    let text = render_text(120, 25, projection, state);
    assert!(text.contains("Unknown window"));
    assert!(text.contains("provider did not report"));
    assert!(!text.contains('█') && !text.contains('░'));
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
