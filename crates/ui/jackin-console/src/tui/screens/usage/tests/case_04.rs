// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn meter_color_grades_by_canonical_quota_state_not_percent() {
    use ratatui::style::Color;

    assert_eq!(
        meter_style(UsageQuotaStateV1::Exhausted).fg,
        Some(Color::Red)
    );
    assert_eq!(meter_style(UsageQuotaStateV1::Error).fg, Some(Color::Red));
    assert_eq!(
        meter_style(UsageQuotaStateV1::Warning).fg,
        Some(Color::Yellow)
    );
    for state in [
        UsageQuotaStateV1::Available,
        UsageQuotaStateV1::NotStarted,
        UsageQuotaStateV1::Unsupported,
        UsageQuotaStateV1::Unavailable,
        UsageQuotaStateV1::NoPermission,
        UsageQuotaStateV1::Unknown,
        UsageQuotaStateV1::NotApplicable,
    ] {
        assert_eq!(
            meter_style(state).fg,
            Some(Color::Green),
            "{state:?} must keep the neutral meter color"
        );
    }
}

#[test]
fn window_group_summary_uses_left_like_windows_and_capsule() {
    let group = UsageMetricGroup {
        group_id: "g".to_owned(),
        rank: 0,
        kind: jackin_protocol::usage_broker::UsageMetricGroupKindV1::Window,
        label: "Weekly".to_owned(),
        scope: jackin_protocol::usage_broker::UsageMetricScopeV1::default(),
        observed_at_epoch: None,
        fetched_at_epoch: 1_800_000_000,
        last_success_at_epoch: Some(1_800_000_000),
        phase: UsageFreshnessPhaseV1::Current,
        is_stale: false,
        quota_state: UsageQuotaStateV1::Available,
        value: jackin_protocol::usage_broker::UsageMetricValueV1::Window {
            remaining_percent: Some(
                jackin_protocol::control::RawPercentPoints::new(73.0).expect("valid percent"),
            ),
            remaining_percent_origin: Some(
                jackin_protocol::control::PercentObservationOrigin::ProviderReported,
            ),
            used_percent: None,
            used_percent_origin: None,
            meter_percent: Some(
                jackin_protocol::control::MeterPercentPoints::new(73.0).expect("valid meter"),
            ),
            meter_side: Some(jackin_protocol::control::UsageMeterSide::Remaining),
            count_quota: None,
            provider_quota_state: None,
            money_used_percent: None,
            period: jackin_protocol::usage_broker::UsageMetricPeriodV1::Calendar {
                granularity: jackin_protocol::usage_broker::UsageCalendarPeriodV1::Weekly,
            },
            unit: None,
        },
        reset_at_epoch: None,
        renews_at_epoch: None,
        issues: Vec::new(),
    };
    assert_eq!(
        metric_group_value_summary(&group).as_deref(),
        Some("73% left (reported) · weekly")
    );
}

#[test]
fn window_detail_preserves_both_reported_sides_and_signed_money_ratio() {
    use jackin_protocol::control::PercentObservationOrigin;

    let window = UsageWindow {
        remaining_percent: Some(
            jackin_protocol::control::RawPercentPoints::new(57.125).expect("valid raw percent"),
        ),
        remaining_percent_origin: Some(PercentObservationOrigin::ProviderReported),
        used_percent: Some(
            jackin_protocol::control::RawPercentPoints::new(43.375).expect("valid raw percent"),
        ),
        used_percent_origin: Some(PercentObservationOrigin::ProviderReported),
        money_used_percent: Some(
            jackin_protocol::control::RawPercentPoints::new(-10.25).expect("valid signed ratio"),
        ),
        ..test_window("weekly", Some(57))
    };

    assert_eq!(
        super::super::raw_percent_note(&window).as_deref(),
        Some("57.13% remaining (reported) · 43.38% used (reported) · money ratio -10.25% used")
    );
}

#[test]
fn window_group_summary_keeps_full_width_typed_request_counts() {
    use jackin_protocol::control::{
        CountQuota, CountQuotaPeriod, CountQuotaProvenance, CountQuotaUnit,
    };
    use jackin_protocol::usage_broker::{UsageMetricPeriodV1, UsageMetricValueV1};

    let count = CountQuota::new(
        Some(u64::MAX),
        Some(u64::MAX),
        Some(u64::MAX - 1),
        CountQuotaUnit::Requests,
        CountQuotaPeriod::Unknown,
        CountQuotaProvenance::ProviderReported,
    );
    let group = UsageMetricGroup {
        group_id: "requests".to_owned(),
        rank: 0,
        kind: jackin_protocol::usage_broker::UsageMetricGroupKindV1::Window,
        label: "Request quota".to_owned(),
        scope: jackin_protocol::usage_broker::UsageMetricScopeV1::default(),
        observed_at_epoch: None,
        fetched_at_epoch: 1_800_000_000,
        last_success_at_epoch: Some(1_800_000_000),
        phase: UsageFreshnessPhaseV1::Current,
        is_stale: false,
        quota_state: UsageQuotaStateV1::Available,
        value: UsageMetricValueV1::Window {
            remaining_percent: None,
            remaining_percent_origin: None,
            used_percent: None,
            used_percent_origin: None,
            meter_percent: None,
            meter_side: None,
            count_quota: Some(count),
            provider_quota_state: None,
            money_used_percent: None,
            period: UsageMetricPeriodV1::Unknown,
            unit: Some("requests".to_owned()),
        },
        reset_at_epoch: None,
        renews_at_epoch: None,
        issues: Vec::new(),
    };

    let summary = metric_group_value_summary(&group).expect("typed count is visible");
    assert_eq!(
        summary,
        format!(
            "{} used · {} limit · {} remaining · requests",
            u64::MAX,
            u64::MAX,
            u64::MAX - 1
        )
    );
}

#[test]
fn count_only_window_summary_shows_every_count_and_explicit_quota_state() {
    use jackin_protocol::control::{
        CountQuota, CountQuotaPeriod, CountQuotaProvenance, CountQuotaUnit,
        PercentObservationOrigin, ProviderQuotaState, RawPercentPoints,
    };

    let mut account = test_account("minimax", "a", "plan");
    account.windows = vec![UsageWindow {
        label: "General · Weekly".to_owned(),
        value: "Unlimited".to_owned(),
        reset: String::new(),
        remaining_percent: Some(RawPercentPoints::new(0.0).unwrap()),
        remaining_percent_origin: Some(PercentObservationOrigin::ProviderReported),
        used_percent: None,
        used_percent_origin: None,
        meter_percent: None,
        meter_side: None,
        count_quota: Some(CountQuota::new(
            Some(1),
            Some(2),
            Some(0),
            CountQuotaUnit::Requests,
            CountQuotaPeriod::Unknown,
            CountQuotaProvenance::ProviderReported,
        )),
        provider_quota_state: Some(ProviderQuotaState::Unlimited),
        money_used_percent: None,
        reset_at_epoch: None,
        quota_state: UsageQuotaStateV1::NotApplicable,
        ..test_window("weekly", None)
    }];
    assert!(account.summary_window().is_some());
    assert_eq!(
        account.min_remaining(),
        None,
        "Unlimited has no bounded rank key"
    );
    let text = render_detail_text(UsageScreenState {
        accounts: vec![account],
        selected: 1,
        detail: false,
        ..UsageScreenState::default()
    });
    for expected in ["Unlimited", "1 used", "2 limit", "0 remaining"] {
        assert!(text.contains(expected), "missing {expected:?}:\n{text}");
    }
    assert_eq!(
        text.matches("Unlimited").count(),
        1,
        "state label must not duplicate:\n{text}"
    );
    assert!(
        text.contains("0% remaining (reported)"),
        "raw source percent stays visible:\n{text}"
    );
}

#[test]
fn count_only_unknown_window_summary_keeps_partial_exact_values_without_meter() {
    use jackin_protocol::control::{
        CountQuota, CountQuotaPeriod, CountQuotaProvenance, CountQuotaUnit,
        PercentObservationOrigin, RawPercentPoints,
    };

    let mut account = test_account("cursor", "a", "plan");
    account.windows = vec![UsageWindow {
        value: String::new(),
        reset: String::new(),
        remaining_percent: Some(RawPercentPoints::new(150.0).unwrap()),
        remaining_percent_origin: Some(PercentObservationOrigin::ProviderReported),
        count_quota: Some(CountQuota::new(
            Some(u64::MAX),
            Some(0),
            None,
            CountQuotaUnit::Unknown,
            CountQuotaPeriod::Unknown,
            CountQuotaProvenance::ProviderReported,
        )),
        ..test_window("Requests", None)
    }];
    assert_eq!(account.windows[0].meter_geometry(), None);
    assert!(account.summary_window().is_some());

    let text = render_detail_text(UsageScreenState {
        accounts: vec![account],
        selected: 1,
        detail: false,
        ..UsageScreenState::default()
    });
    for expected in [
        "18446744073709551615 used",
        "0 limit",
        "150% remaining (reported)",
    ] {
        assert!(text.contains(expected), "missing {expected:?}:\n{text}");
    }
}

#[test]
fn metric_group_state_only_summary_is_visible() {
    use jackin_protocol::control::ProviderQuotaState;
    use jackin_protocol::usage_broker::UsageMetricValueV1;

    let mut group = window_metric_group("Unlimited window", None, 1_800_000_000);
    if let UsageMetricValueV1::Window {
        provider_quota_state,
        ..
    } = &mut group.value
    {
        *provider_quota_state = Some(ProviderQuotaState::Unlimited);
    }
    assert_eq!(
        metric_group_value_summary(&group).as_deref(),
        Some("Unlimited")
    );
}

#[test]
fn detail_toggle_renders_summary_vs_full_bodies() {
    let mut account = test_account("openai", "a", "work");
    account.windows = vec![UsageWindow {
        quota_state: UsageQuotaStateV1::Warning,
        pace_label: Some("on pace".to_owned()),
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
        summary.contains("Session: 5% left"),
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
    assert_eq!(screen.selected, 1);
    assert_eq!(screen.selected_id, Some("openai:a".to_owned()));

    press_key(&mut manager, KeyCode::Char('f'));
    let screen = manager.usage.screen.as_ref().unwrap();
    assert_eq!(screen.filter, UsageFilter::Issues);
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
    assert_eq!(mixed.min_remaining(), Some(5.0));

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

    state.accounts[2].windows = vec![test_window("w", Some(0))];
    assert_eq!(state.most_constrained_selected(), Some(3));
}

#[test]
fn capacity_finder_empty_states_post_notice_without_moving() {
    use super::UsageFilter;

    let mut empty = UsageScreenState::open_with_snapshot(snapshot(Vec::new(), None));
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
