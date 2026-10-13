// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn s8_resize_pair_keeps_identity_and_first_reset() {
    let mut account = test_account("openai", "a", "work");
    account.provider = "OpenAI".to_owned();
    account.windows = vec![UsageWindow {
        label: "5h".to_owned(),
        value: "10% left".to_owned(),
        reset: "in 2h".to_owned(),
        ..test_window("5h", Some(10))
    }];
    let manager = s8_manager(UsageScreenState {
        accounts: vec![account],
        selected: 1,
        selected_id: Some("openai:a".to_owned()),
        detail: true,
        ..UsageScreenState::default()
    });
    for (width, height) in [(80, 24), (40, 20), (120, 40)] {
        let text = s8_render_full(&manager, width, height);
        assert!(
            text.contains("OpenAI"),
            "identity lost at {width}x{height}:\n{text}"
        );
        assert!(
            text.contains("work"),
            "identity lost at {width}x{height}:\n{text}"
        );
        assert!(
            text.contains("5h"),
            "window lost at {width}x{height}:\n{text}"
        );
        assert!(
            text.contains("10% left"),
            "first detail lost at {width}x{height}:\n{text}"
        );
        assert!(
            text.contains("in 2h"),
            "reset lost at {width}x{height}:\n{text}"
        );
    }
}

#[test]
fn s8_scroll_moves_overview_content() {
    let accounts = (0..12)
        .map(|n| {
            let mut account = test_account("openai", &format!("a{n}"), &format!("account-{n}"));
            account.provider = "OpenAI".to_owned();
            account
        })
        .collect::<Vec<_>>();
    let manager = s8_manager(UsageScreenState {
        accounts,
        ..UsageScreenState::default()
    });
    let top = s8_render_full(&manager, 80, 24);
    assert!(top.contains("account-0"), "{top}");

    let mut scrolled = manager.usage.screen.as_ref().unwrap().clone();
    scrolled.scroll = 25;
    let manager = s8_manager(scrolled);
    let moved = s8_render_full(&manager, 80, 24);
    assert!(!moved.contains("account-0"), "{moved}");
    assert!(moved.contains("account-"), "{moved}");
}

#[test]
fn complete_publication_survives_refresh_cache_reopen_and_render() {
    let (mut projection, _) = metric_group_projection_fixture();
    projection.generated_at_epoch = TEST_NOW_EPOCH - 600;
    projection.issues[0].message = "independent publication warning".to_owned();
    projection.providers[0].accounts[0].windows[0].value_label = "independent 120% used".to_owned();
    let publication = UsageScreenState::from_projection(&projection);
    let mut state = UsageScreenState::open_with_snapshot(UsageScreenState::default());
    let plan = state.next_refresh_plan_if_due(Instant::now()).unwrap();
    state.begin_refresh(crate::tui::runtime::ready_blocking_subscription((
        plan.generation,
        Ok(publication),
    )));
    let completed = state.poll_refresh().unwrap().unwrap();
    let cached = completed.clone();
    state.apply_refresh(completed, Instant::now());
    assert_eq!(state.canonical_projection.as_ref(), Some(&projection));
    assert_eq!(state.generated_at_epoch, Some(TEST_NOW_EPOCH - 600));
    assert_eq!(
        state.projection_issues[0].message,
        "independent publication warning"
    );
    let rendered = render_detail_text(state.clone());
    assert!(rendered.contains("Snapshot  10m ago"), "{rendered}");
    assert!(
        rendered.contains("independent publication warning"),
        "{rendered}"
    );
    assert!(rendered.contains("independent 120% used"), "{rendered}");
    let list = render_list_text(state.clone());
    assert!(list.contains("independent 120% used"), "{list}");
    assert!(!list.contains("0% left"), "{list}");
    let reopened = UsageScreenState::open_with_snapshot(cached);
    assert_eq!(reopened.canonical_projection.as_ref(), Some(&projection));
    assert!(render_detail_text(reopened).contains("independent publication warning"));

    let mut cleared = projection;
    cleared.generated_at_epoch = TEST_NOW_EPOCH;
    cleared.issues.clear();
    state.apply_refresh(UsageScreenState::from_projection(&cleared), Instant::now());
    assert!(state.projection_issues.is_empty());
    assert_eq!(state.generated_at_epoch, Some(TEST_NOW_EPOCH));
    assert!(!render_detail_text(state.clone()).contains("independent publication warning"));
    let mut unlimited = cleared;
    let window = &mut unlimited.providers[0].accounts[0].windows[0];
    window.quota_state = UsageQuotaStateV1::NotApplicable;
    window.value_label = "Independent unlimited quota".to_owned();
    window.remaining_percent = None;
    window.remaining_raw_percent = None;
    window.used_percent = None;
    window.used_raw_percent = None;
    state.apply_refresh(
        UsageScreenState::from_projection(&unlimited),
        Instant::now(),
    );
    let list = render_list_text(state);
    assert!(list.contains("Independent unlimited quota"), "{list}");
    assert!(!list.contains('█') && !list.contains('░'), "{list}");
}

#[test]
fn empty_failed_publication_renders_diagnostics_in_both_panels() {
    let (mut projection, _) = metric_group_projection_fixture();
    projection.providers.clear();
    projection.issues[0].message = "independent empty broker failure".to_owned();
    let mut state = UsageScreenState::from_projection(&projection);
    state.apply_refresh_error("Usage unavailable: disconnected".to_owned(), Instant::now());
    for rendered in [render_detail_text(state.clone()), render_list_text(state)] {
        assert!(
            rendered.contains("independent empty broker failure"),
            "{rendered}"
        );
        assert!(
            rendered.contains("Usage unavailable: disconnected"),
            "{rendered}"
        );
        assert!(!rendered.contains("No providers configured"), "{rendered}");
    }
}

#[test]
fn canonical_refreshing_publication_keeps_loading_after_worker_completion() {
    use jackin_protocol::usage_broker::UsageProjectionRefreshStateV1;
    let (mut projection, _) = metric_group_projection_fixture();
    projection.providers.clear();
    projection.issues.clear();
    projection.refresh_state = UsageProjectionRefreshStateV1::Refreshing;
    let mut state = UsageScreenState::default();
    state.apply_refresh(
        UsageScreenState::from_projection(&projection),
        Instant::now(),
    );
    assert!(!state.refresh_in_flight());
    assert!(state.loading());
    for rendered in [render_detail_text(state.clone()), render_list_text(state)] {
        assert!(rendered.contains("Refreshing usage…"), "{rendered}");
        assert!(!rendered.contains("No providers configured"), "{rendered}");
    }
}

#[test]
fn provider_discovery_diagnostics_survive_without_account_rows() {
    let (mut projection, _) = metric_group_projection_fixture();
    projection.issues.clear();
    projection.providers[0].accounts.clear();
    projection.providers[0].issues[0].message =
        "independent provider configuration failure".to_owned();
    let state = UsageScreenState::from_projection(&projection);
    assert!(state.accounts.is_empty());
    for rendered in [render_detail_text(state.clone()), render_list_text(state)] {
        assert!(
            rendered.contains("independent provider configuration failure"),
            "{rendered}"
        );
        assert!(!rendered.contains("No providers configured"), "{rendered}");
    }
}

#[test]
fn filtered_accounts_cannot_hide_provider_publication_diagnostics() {
    let (mut projection, _) = metric_group_projection_fixture();
    projection.issues.clear();
    projection.providers[0].issues[0].message = "independent hidden-provider diagnostic".to_owned();
    let mut state = UsageScreenState::from_projection(&projection);
    state.filter = UsageFilter::Stale;
    assert!(state.visible_order().is_empty());
    let rendered = render_detail_text(state);
    assert!(
        rendered.contains("independent hidden-provider diagnostic"),
        "{rendered}"
    );
}

#[test]
fn manual_refresh_join_after_success_keeps_periodic_request_non_forced() {
    for claim_before_completion in [false, true] {
        assert_manual_refresh_join_preserves_periodic_cadence(false, claim_before_completion);
    }
}

#[test]
fn manual_refresh_join_after_error_keeps_periodic_request_non_forced() {
    for claim_before_completion in [false, true] {
        assert_manual_refresh_join_preserves_periodic_cadence(true, claim_before_completion);
    }
}
