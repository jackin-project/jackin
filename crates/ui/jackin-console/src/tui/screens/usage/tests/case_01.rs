// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_protocol::control::{MeterPercentPoints, PercentObservationOrigin, UsageMeterSide};

#[test]
fn summary_window_selects_first_ranked_metered_limit() {
    let mut account = test_account("antigravity", "a", "pilot");
    account.windows = vec![
        UsageWindow {
            rank: 0,
            category: UsageWindowCategoryV1::Session,
            ..test_window("session", Some(73))
        },
        UsageWindow {
            rank: 1,
            category: UsageWindowCategoryV1::LongRange,
            ..test_window("weekly", Some(41))
        },
        UsageWindow {
            rank: 2,
            category: UsageWindowCategoryV1::Other,
            ..test_window("other", Some(12))
        },
    ];
    assert_eq!(
        account.summary_window().map(|window| window.label.as_str()),
        Some("weekly")
    );

    account.windows = vec![
        UsageWindow {
            rank: 0,
            category: UsageWindowCategoryV1::LongRange,
            ..test_window("unknown", None)
        },
        UsageWindow {
            rank: 1,
            category: UsageWindowCategoryV1::Session,
            ..test_window("session", Some(50))
        },
    ];
    assert_eq!(
        account.summary_window().map(|window| window.label.as_str()),
        Some("session")
    );

    account.windows = vec![test_window("unknown", None)];
    assert_eq!(account.summary_window(), None);
}

#[test]
fn usage_meter_scales_to_remaining_percentage() {
    let geometry = |percent, side| Some((MeterPercentPoints::new(percent).unwrap(), side));
    assert_eq!(
        meter_line(10, geometry(50.0, UsageMeterSide::Remaining)),
        Some("  █████░░░░░".to_owned())
    );
    assert_eq!(
        meter_line(10, geometry(0.0, UsageMeterSide::Remaining)),
        Some("  ░░░░░░░░░░".to_owned())
    );
    assert_eq!(
        meter_line(10, geometry(100.0, UsageMeterSide::Remaining)),
        Some("  ██████████".to_owned())
    );
    assert_eq!(
        meter_line(10, geometry(43.375, UsageMeterSide::Used)),
        Some("  ████░░░░░░".to_owned())
    );
}

#[test]
fn usage_meter_renders_no_bar_for_unknown_percent() {
    assert_eq!(meter_line(10, None), None);
}

#[test]
fn usage_window_keeps_continuous_used_geometry_and_origin() {
    let window = UsageWindow {
        remaining_percent: None,
        used_percent: Some(jackin_protocol::control::RawPercentPoints::new(43.375).unwrap()),
        used_percent_origin: Some(PercentObservationOrigin::ProviderReported),
        meter_percent: Some(MeterPercentPoints::new(43.375).unwrap()),
        meter_side: Some(UsageMeterSide::Used),
        ..test_window_points("weekly", None)
    };
    assert_eq!(
        window.meter_percent().map(MeterPercentPoints::get),
        Some(43.375)
    );
    assert_eq!(
        window
            .used_percent
            .map(jackin_protocol::control::RawPercentPoints::get),
        Some(43.375)
    );
    assert_eq!(
        window.used_percent_origin,
        Some(PercentObservationOrigin::ProviderReported)
    );
    let mut account = test_account("openai", "a", "work");
    account.windows = vec![window];
    assert_eq!(account.min_remaining(), Some(56.625));
    let unknown = test_window("weekly", None);
    assert_eq!(unknown.meter_percent(), None);

    let overage = UsageWindow {
        used_percent: Some(jackin_protocol::control::RawPercentPoints::new(120.25).unwrap()),
        used_percent_origin: Some(PercentObservationOrigin::ProviderReported),
        meter_percent: Some(MeterPercentPoints::new(100.0).unwrap()),
        meter_side: Some(UsageMeterSide::Used),
        ..test_window_points("overage", None)
    };
    let mut overage_account = test_account("openai", "overage", "overage");
    overage_account.windows = vec![overage];
    assert_eq!(overage_account.min_remaining(), Some(-20.25));
}

#[test]
fn remaining_sort_uses_raw_overage_when_meter_geometry_ties() {
    use super::UsageSort;

    let mut less_over = test_account("provider", "less", "less over");
    less_over.windows = vec![UsageWindow {
        used_percent: Some(jackin_protocol::control::RawPercentPoints::new(120.25).unwrap()),
        used_percent_origin: Some(PercentObservationOrigin::ProviderReported),
        meter_percent: Some(MeterPercentPoints::new(100.0).unwrap()),
        meter_side: Some(UsageMeterSide::Used),
        ..test_window_points("Session", None)
    }];
    let mut more_over = test_account("provider", "more", "more over");
    more_over.windows = vec![UsageWindow {
        used_percent: Some(jackin_protocol::control::RawPercentPoints::new(120.5).unwrap()),
        used_percent_origin: Some(PercentObservationOrigin::ProviderReported),
        meter_percent: Some(MeterPercentPoints::new(100.0).unwrap()),
        meter_side: Some(UsageMeterSide::Used),
        ..test_window_points("Session", None)
    }];
    assert_eq!(
        less_over.windows[0].meter_geometry(),
        more_over.windows[0].meter_geometry()
    );

    let state = UsageScreenState {
        accounts: vec![less_over, more_over],
        sort: UsageSort::Remaining,
        ..UsageScreenState::default()
    };
    assert_eq!(state.visible_order(), vec![1, 0]);
}

#[test]
fn remaining_sort_uses_raw_request_count_ratio_when_meter_geometry_ties() {
    use super::UsageSort;
    use jackin_protocol::control::{
        CountQuota, CountQuotaPeriod, CountQuotaProvenance, CountQuotaUnit,
    };

    let request_window = |label: &str, remaining: u64| UsageWindow {
        count_quota: Some(CountQuota::new(
            None,
            Some(1_000),
            Some(remaining),
            CountQuotaUnit::Requests,
            CountQuotaPeriod::Unknown,
            CountQuotaProvenance::ProviderReported,
        )),
        meter_percent: Some(MeterPercentPoints::new(100.0).expect("bounded meter")),
        meter_side: Some(UsageMeterSide::Remaining),
        remaining_percent: None,
        remaining_percent_origin: None,
        ..test_window_points(label, None)
    };

    let mut less_over = test_account("provider", "less", "less over");
    less_over.windows = vec![request_window("Requests", 1_001)];
    let mut more_over = test_account("provider", "more", "more over");
    more_over.windows = vec![request_window("Requests", 1_002)];
    assert_eq!(
        less_over.windows[0].meter_geometry(),
        more_over.windows[0].meter_geometry()
    );

    let state = UsageScreenState {
        accounts: vec![less_over, more_over],
        sort: UsageSort::Remaining,
        ..UsageScreenState::default()
    };
    assert_eq!(state.visible_order(), vec![0, 1]);
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
    let mut state = UsageScreenState::open_with_snapshot(snapshot(
        vec![
            test_account("openai", "a", "work"),
            test_account("anthropic", "b", "personal"),
        ],
        None,
    ));
    state.move_selection(1);
    assert_eq!(state.selected_id, Some("openai:a".to_owned()));

    let mut renamed = test_account("openai", "a", "work-renamed");
    renamed.provider = "OpenAI Renamed".to_owned();
    state.apply_refresh(
        snapshot(
            vec![test_account("anthropic", "b", "personal"), renamed],
            None,
        ),
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
    let mut state = UsageScreenState::open_with_snapshot(snapshot(
        vec![
            test_account("openai", "a", "work"),
            test_account("anthropic", "b", "personal"),
        ],
        None,
    ));
    state.move_selection(2);
    assert_eq!(state.selected_id, Some("anthropic:b".to_owned()));

    state.apply_refresh(
        snapshot(vec![test_account("openai", "a", "work")], None),
        now,
    );
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
    let mut state = UsageScreenState::open_with_snapshot(snapshot(
        vec![test_account("openai", "a", "work")],
        None,
    ));
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
    let state = UsageScreenState::open_with_snapshot(snapshot(Vec::new(), None));
    assert!(state.refresh_due);
    assert!(!state.heartbeat_due(Instant::now()));

    let mut state = state;
    let completed = Instant::now()
        .checked_sub(USAGE_HEARTBEAT_INTERVAL + Duration::from_secs(1))
        .expect("heartbeat interval fits in uptime");
    state.apply_refresh(snapshot(Vec::new(), None), completed);
    assert!(state.heartbeat_due(Instant::now()));

    let mut fresh = UsageScreenState::open_with_snapshot(snapshot(Vec::new(), None));
    fresh.apply_refresh(snapshot(Vec::new(), None), Instant::now());
    assert!(!fresh.heartbeat_due(Instant::now()));
}

#[test]
fn poll_refresh_delivers_ready_outcome_and_clears_in_flight() {
    let mut state = UsageScreenState::open_with_snapshot(snapshot(Vec::new(), None));
    assert!(!state.refresh_in_flight());
    let plan = state
        .next_refresh_plan_if_due(Instant::now())
        .expect("open marks a refresh due");
    state.begin_refresh(crate::tui::runtime::ready_blocking_subscription((
        plan.generation,
        Ok(snapshot(
            vec![test_account("openai", "a", "work")],
            Some("n".to_owned()),
        )),
    )));
    assert!(state.refresh_in_flight());
    let outcome = state.poll_refresh().expect("ready outcome");
    let snapshot = outcome.expect("ok outcome");
    assert_eq!(snapshot.accounts.len(), 1);
    assert_eq!(snapshot.notice, Some("n".to_owned()));
    assert!(!state.refresh_in_flight());
}

#[test]
fn poll_refresh_drops_stale_generations() {
    let mut state = UsageScreenState::open_with_snapshot(snapshot(Vec::new(), None));
    let plan = state
        .next_refresh_plan_if_due(Instant::now())
        .expect("open marks a refresh due");
    state.begin_refresh(crate::tui::runtime::ready_blocking_subscription((
        plan.generation.wrapping_add(1),
        Ok(snapshot(vec![test_account("openai", "a", "work")], None)),
    )));
    assert!(state.poll_refresh().is_none(), "stale outcome dropped");
    assert!(!state.refresh_in_flight());
    assert!(state.accounts.is_empty());
}

#[test]
fn refresh_plan_joins_in_flight_work_without_queueing() {
    let mut state = UsageScreenState::open_with_snapshot(snapshot(Vec::new(), None));
    let plan = state
        .next_refresh_plan_if_due(Instant::now())
        .expect("open marks a refresh due");
    assert!(!plan.force);
    state.begin_refresh(crate::tui::runtime::ready_blocking_subscription((
        plan.generation,
        Ok(snapshot(Vec::new(), None)),
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
    let mut state = UsageScreenState::open_with_snapshot(snapshot(Vec::new(), None));
    let open = state
        .next_refresh_plan_if_due(Instant::now())
        .expect("open marks a refresh due");
    assert!(!open.force, "open subscribes without forcing");
    state.apply_refresh(snapshot(Vec::new(), None), Instant::now());
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

    state.apply_refresh(snapshot(Vec::new(), None), Instant::now());
    let completed = Instant::now()
        .checked_sub(USAGE_HEARTBEAT_INTERVAL + Duration::from_secs(1))
        .expect("heartbeat interval fits in uptime");
    state.apply_refresh(snapshot(Vec::new(), None), completed);
    let heartbeat = state
        .next_refresh_plan_if_due(Instant::now())
        .expect("heartbeat is due");
    assert!(!heartbeat.force, "periodic refresh honors broker cadence");
}

#[test]
fn screen_clone_drops_in_flight_refresh_but_keeps_value_state() {
    let mut state = UsageScreenState::open_with_snapshot(snapshot(
        vec![test_account("openai", "a", "work")],
        None,
    ));
    let plan = state
        .next_refresh_plan_if_due(Instant::now())
        .expect("open marks a refresh due");
    state.begin_refresh(crate::tui::runtime::ready_blocking_subscription((
        plan.generation,
        Ok(snapshot(Vec::new(), None)),
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
    let state = UsageScreenState::open_with_snapshot(snapshot(
        vec![test_account("openai", "a", "work")],
        None,
    ));
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
    manager.usage.screen = Some(UsageScreenState::open_with_snapshot(snapshot(
        Vec::new(),
        None,
    )));
    manager
        .usage
        .screen
        .as_mut()
        .unwrap()
        .apply_refresh(snapshot(Vec::new(), None), Instant::now());
    assert!(!manager.usage.screen.as_ref().unwrap().refresh_due);

    let key = KeyEvent {
        code: KeyCode::Char('r'),
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: KeyEventState::empty(),
    };
    handle_key(&mut manager, key);
    let screen = manager.usage.screen.as_ref().unwrap();
    assert!(screen.refresh_due);
    assert!(screen.force_refresh_pending);
}

#[test]
fn freshness_age_label_covers_phases_and_ages() {
    let now = TEST_NOW_EPOCH;
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
