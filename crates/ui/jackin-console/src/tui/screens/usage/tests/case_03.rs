// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
        .draw(|f| render_detail(f, f.area(), &manager, TEST_NOW_EPOCH))
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
            .draw(|f| render_at(f, f.area(), &manager, TEST_NOW_EPOCH))
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
        Ok(snapshot(Vec::new(), None)),
    )));
    // Keep in flight: poll nothing, render while pending.
    manager.usage.screen = Some(state);

    let backend = TestBackend::new(80, 25);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| render_detail(f, f.area(), &manager, TEST_NOW_EPOCH))
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
        .draw(|f| render_detail(f, f.area(), &manager, TEST_NOW_EPOCH))
        .unwrap();

    let text = backend_text(&terminal);
    assert!(text.contains("mystery window"));
    assert!(text.contains("provider did not report"));
    assert!(
        !text.contains('█') && !text.contains('░'),
        "unknown quota must not render a bar:\n{text}"
    );
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
        .draw(|f| render_detail(f, f.area(), &manager, TEST_NOW_EPOCH))
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
        .draw(|f| render_detail(f, f.area(), &manager, TEST_NOW_EPOCH))
        .unwrap();
    let overview = backend_text(&terminal);
    assert_overview_group_rows(&overview);
    assert!(
        !overview.contains("remaining"),
        "unreported remaining must not render:\n{overview}"
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
