// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn labels_align_with_capsule_tab_vocabulary() {
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

#[test]
fn s8_enter_toggles_detail_and_flips_hint() {
    let mut account = test_account("openai", "a", "work");
    account.provider = "OpenAI".to_owned();
    let mut manager = s8_manager(UsageScreenState {
        accounts: vec![account],
        selected: 1,
        selected_id: Some("openai:a".to_owned()),
        ..UsageScreenState::default()
    });

    s8_press(&mut manager, crossterm::event::KeyCode::Enter);
    assert!(manager.usage.screen.as_ref().unwrap().detail);
    let detail = s8_render_full(&manager, 80, 24);
    assert!(detail.contains("Enter for summary"), "{detail}");

    s8_press(&mut manager, crossterm::event::KeyCode::Enter);
    assert!(!manager.usage.screen.as_ref().unwrap().detail);
    let summary = s8_render_full(&manager, 80, 24);
    assert!(summary.contains("Enter for full detail"), "{summary}");
}

#[test]
fn s8_jk_and_arrows_traverse_and_clamp() {
    use crossterm::event::KeyCode;
    let mut manager = s8_manager(UsageScreenState {
        accounts: vec![
            test_account("openai", "a", "a"),
            test_account("anthropic", "b", "b"),
        ],
        ..UsageScreenState::default()
    });

    s8_press(&mut manager, KeyCode::Char('j'));
    s8_press(&mut manager, KeyCode::Down);
    assert_eq!(manager.usage.screen.as_ref().unwrap().selected, 2);
    s8_press(&mut manager, KeyCode::Char('j'));
    assert_eq!(manager.usage.screen.as_ref().unwrap().selected, 2);
    s8_press(&mut manager, KeyCode::Char('k'));
    s8_press(&mut manager, KeyCode::Up);
    s8_press(&mut manager, KeyCode::Up);
    let screen = manager.usage.screen.as_ref().unwrap();
    assert_eq!(screen.selected, 0);
    assert_eq!(screen.selected_id, None);
}

#[test]
fn s8_sort_filter_constrained_keys_reanchor_by_stable_id() {
    use crossterm::event::KeyCode;
    use jackin_protocol::usage_broker::{
        UsageIssueRecoverabilityV1, UsageIssueScopeV1, UsageIssueV1,
    };
    let mut low = test_account("anthropic", "b", "home");
    low.windows = vec![test_window("weekly", Some(10))];
    low.issues = vec![UsageIssueV1 {
        code: "quota_degraded".to_owned(),
        scope: UsageIssueScopeV1::Account,
        recoverability: UsageIssueRecoverabilityV1::Retryable,
        message: "quota degraded".to_owned(),
        retry_at_epoch: None,
    }];
    let mut mid = test_account("openai", "c", "lab");
    mid.windows = vec![test_window("weekly", Some(50))];
    let mut high = test_account("openai", "a", "work");
    high.windows = vec![test_window("weekly", Some(73))];
    let mut manager = s8_manager(UsageScreenState {
        accounts: vec![high, low, mid],
        ..UsageScreenState::default()
    });

    // `c` jumps to the most constrained visible account (10% left).
    s8_press(&mut manager, KeyCode::Char('c'));
    let screen = manager.usage.screen.as_ref().unwrap();
    assert_eq!(screen.selected, 2);
    assert_eq!(screen.selected_id, Some("anthropic:b".to_owned()));

    // `s` sorts by remaining; the selected row follows its stable id.
    s8_press(&mut manager, KeyCode::Char('s'));
    let screen = manager.usage.screen.as_ref().unwrap();
    assert_eq!(screen.sort, UsageSort::Remaining);
    assert_eq!(screen.selected, 1);
    assert_eq!(screen.selected_id, Some("anthropic:b".to_owned()));

    // `f` filters to issues; the selected row (which has one) stays put.
    s8_press(&mut manager, KeyCode::Char('f'));
    let screen = manager.usage.screen.as_ref().unwrap();
    assert_eq!(screen.filter, UsageFilter::Issues);
    assert_eq!(screen.selected, 1);

    // Move to Overview, then filter to a set hiding nothing selected: the
    // parked id restores once the filter clears.
    s8_press(&mut manager, KeyCode::Char('k'));
    s8_press(&mut manager, KeyCode::Char('f'));
    let screen = manager.usage.screen.as_ref().unwrap();
    assert_eq!(screen.filter, UsageFilter::Stale);
    assert_eq!(screen.selected, 0);
    s8_press(&mut manager, KeyCode::Char('f'));
    let screen = manager.usage.screen.as_ref().unwrap();
    assert_eq!(screen.filter, UsageFilter::All);
}

#[test]
fn s8_page_keys_scroll_and_saturate() {
    use crossterm::event::KeyCode;
    let mut manager = s8_manager(UsageScreenState {
        accounts: vec![test_account("openai", "a", "a")],
        ..UsageScreenState::default()
    });

    s8_press(&mut manager, KeyCode::PageDown);
    s8_press(&mut manager, KeyCode::PageDown);
    s8_press(&mut manager, KeyCode::PageDown);
    assert_eq!(manager.usage.screen.as_ref().unwrap().scroll, 15);
    s8_press(&mut manager, KeyCode::PageUp);
    assert_eq!(manager.usage.screen.as_ref().unwrap().scroll, 10);
    for _ in 0..10 {
        s8_press(&mut manager, KeyCode::PageUp);
    }
    assert_eq!(manager.usage.screen.as_ref().unwrap().scroll, 0);
}

#[test]
fn s8_esc_and_q_close_route() {
    use crossterm::event::KeyCode;
    let mut manager = s8_manager(UsageScreenState::open_with_snapshot(snapshot(
        vec![test_account("openai", "a", "a")],
        None,
    )));

    s8_press(&mut manager, KeyCode::Esc);
    assert!(!manager.usage.visible);
    manager.usage.visible = true;
    s8_press(&mut manager, KeyCode::Char('q'));
    assert!(!manager.usage.visible);
}

#[test]
fn s8_unknown_key_is_noop() {
    use crossterm::event::KeyCode;
    let mut manager = s8_manager(UsageScreenState::open_with_snapshot(snapshot(
        vec![test_account("openai", "a", "a")],
        None,
    )));
    let before = manager.usage.screen.as_ref().unwrap().clone();
    s8_press(&mut manager, KeyCode::Char('z'));
    s8_press(&mut manager, KeyCode::F(5));
    assert_eq!(manager.usage.screen.as_ref().unwrap(), &before);
    assert!(manager.usage.visible);
}

#[test]
fn s8_uppercase_r_refreshes_like_lowercase() {
    use crossterm::event::KeyCode;
    for code in [KeyCode::Char('r'), KeyCode::Char('R')] {
        let mut manager = s8_manager(UsageScreenState::open_with_snapshot(snapshot(
            vec![test_account("openai", "a", "a")],
            None,
        )));
        manager.usage.screen.as_mut().unwrap().apply_refresh(
            snapshot(vec![test_account("openai", "a", "a")], None),
            Instant::now(),
        );
        assert!(!manager.usage.screen.as_ref().unwrap().refresh_due);
        s8_press(&mut manager, code);
        let screen = manager.usage.screen.as_ref().unwrap();
        assert!(screen.refresh_due, "key {code:?} must mark refresh due");
        assert!(screen.force_refresh_pending);
    }
}

#[test]
fn s8_refresh_resets_scroll() {
    let mut state = UsageScreenState::open_with_snapshot(snapshot(
        vec![
            test_account("openai", "a", "a"),
            test_account("anthropic", "b", "b"),
        ],
        None,
    ));
    state.move_selection(1);
    state.scroll = 40;
    state.apply_refresh(
        snapshot(
            vec![
                test_account("openai", "a", "a"),
                test_account("anthropic", "b", "b"),
            ],
            None,
        ),
        Instant::now(),
    );
    assert_eq!(state.scroll, 0);
    assert_eq!(state.selected, 1);
    assert_eq!(state.selected_id, Some("openai:a".to_owned()));
}

#[test]
fn s8_removal_while_detail_open_returns_to_overview() {
    let mut state = UsageScreenState::open_with_snapshot(snapshot(
        vec![
            test_account("openai", "a", "work"),
            test_account("anthropic", "b", "personal"),
        ],
        None,
    ));
    state.move_selection(2);
    state.detail = true;
    state.apply_refresh(
        snapshot(vec![test_account("openai", "a", "work")], None),
        Instant::now(),
    );
    assert_eq!(state.selected, 0);
    assert_eq!(state.selected_id, None);
    assert_eq!(
        state.notice,
        Some("Previously selected account unavailable; showing Overview".to_owned())
    );

    let manager = s8_manager(state);
    let text = s8_render_full(&manager, 80, 24);
    assert!(text.contains("Overview"), "{text}");
    // The notice wraps across rows at 80 columns; the head stays contiguous.
    assert!(
        text.contains("Previously selected account unavailable"),
        "{text}"
    );
    // The detail flag is a sticky view-mode preference: removal parks on
    // Overview without flipping it, so the next selected account still
    // opens in the operator's preferred mode.
    assert!(manager.usage.screen.as_ref().unwrap().detail);
}

#[test]
fn s8_empty_loading_renders_refreshing_not_unconfigured() {
    // Open path: refresh due but worker not started yet.
    let manager = s8_manager(UsageScreenState::open_with_snapshot(snapshot(
        Vec::new(),
        None,
    )));
    assert!(manager.usage.screen.as_ref().unwrap().loading());
    let text = s8_render_full(&manager, 80, 24);
    assert!(text.contains("Refreshing usage…"), "{text}");
    assert!(!text.contains("No providers configured"), "{text}");

    // In-flight refresh over an empty cache: same loading line.
    let mut manager = s8_manager(UsageScreenState::open_with_snapshot(snapshot(
        Vec::new(),
        None,
    )));
    let screen = manager.usage.screen.as_mut().unwrap();
    let plan = screen
        .next_refresh_plan_if_due(Instant::now())
        .expect("open marks a refresh due");
    screen.begin_refresh(crate::tui::runtime::ready_blocking_subscription((
        plan.generation,
        Ok(snapshot(Vec::new(), None)),
    )));
    let text = s8_render_full(&manager, 80, 24);
    assert!(text.contains("Refreshing usage…"), "{text}");
    assert!(!text.contains("No providers configured"), "{text}");

    // Completed empty refresh: genuinely nothing configured.
    let mut done = UsageScreenState::open_with_snapshot(snapshot(Vec::new(), None));
    done.apply_refresh(snapshot(Vec::new(), None), Instant::now());
    assert!(!done.loading());
    let manager = s8_manager(done);
    let text = s8_render_full(&manager, 80, 24);
    assert!(text.contains("No providers configured."), "{text}");
    assert!(text.contains("Press R to refresh."), "{text}");
    assert!(!text.contains("Refreshing usage…"), "{text}");
}

#[test]
fn s8_error_notice_renders_without_moving_selection() {
    let mut state = UsageScreenState::open_with_snapshot(snapshot(
        vec![test_account("openai", "a", "work")],
        None,
    ));
    state.move_selection(1);
    state.apply_refresh_error("Usage unavailable: boom".to_owned(), Instant::now());
    let manager = s8_manager(state);
    let text = s8_render_full(&manager, 80, 24);
    assert!(text.contains("Usage unavailable: boom"), "{text}");
    assert!(text.contains("work"), "{text}");
    let screen = manager.usage.screen.as_ref().unwrap();
    assert_eq!(screen.selected, 1);
    assert_eq!(screen.selected_id, Some("openai:a".to_owned()));
}

#[test]
fn s8_long_unicode_labels_render() {
    let mut account = test_account("anthropic", "u1", "work-巴黎-🚀-memo");
    account.provider = "Anthropic / Claude".to_owned();
    account.account.push_str(&"·很长的账户备注".repeat(12));
    account.windows = vec![UsageWindow {
        label: "每周配额 weekly 🚀".to_owned(),
        value: "73% left".to_owned(),
        reset: "resets 明天".to_owned(),
        ..test_window("weekly", Some(73))
    }];
    let manager = s8_manager(UsageScreenState {
        accounts: vec![account],
        selected: 1,
        selected_id: Some("anthropic:u1".to_owned()),
        detail: true,
        ..UsageScreenState::default()
    });
    let text = s8_render_full(&manager, 80, 24);
    assert!(text.contains("Anthropic / Claude"), "{text}");
    assert!(text.contains("🚀"), "{text}");
    assert!(text.contains("73% left"), "{text}");
    // Wide CJK cells dump with spacer cells in the symbol-per-cell harness;
    // squeeze whitespace before asserting the underlying content survived.
    let squeezed: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(squeezed.contains("巴黎"), "{text}");
    assert!(squeezed.contains("每周配额"), "{text}");
    assert!(squeezed.contains("很长的账户备注"), "{text}");
}
