// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn usage_dialog_provider_tabs_are_clickable() {
    let mut d = Dialog::new_usage_with_tab(usage_view_fixture(), UsageDialogTab::Overview);
    let (tab_row, tab_col) = usage_tab_text_position(&d, 32, 120, "Anthropic");

    assert!(d.clickable_at(tab_row, tab_col, 32, 120, None));
    match d.handle_click(tab_row, tab_col, 32, 120, None) {
        DialogAction::SwitchUsageProvider {
            provider_label,
            account_id,
        } => {
            assert_eq!(provider_label, "Claude");
            assert_eq!(account_id, "test-tab-claude");
        }
        other => panic!("expected provider switch, got {other:?}"),
    }
}

#[test]
fn usage_dialog_provider_tab_hover_uses_shared_tab_hover_color() {
    let mut d = Dialog::new_usage_with_tab(usage_view_fixture(), UsageDialogTab::Overview);
    let (tab_row, tab_col) = usage_tab_text_position(&d, 32, 120, "Anthropic");

    assert!(d.set_usage_tab_hover(tab_row, tab_col, 32, 120));

    let snapshot = d.to_ratatui_snapshot(None);
    let rect = d.box_rect(32, 120);
    let backend = TestBackend::new(120, 32);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            crate::tui::components::dialog_widgets::render_dialog_ratatui(frame, rect, &snapshot);
        })
        .unwrap();

    assert!(
        terminal.backend().buffer()[(tab_col, tab_row)]
            .modifier
            .contains(ratatui::style::Modifier::UNDERLINED)
    );
}

#[test]
fn usage_dialog_overview_tab_click_selects_overview() {
    let mut d = Dialog::new_usage(usage_view_fixture());
    let (tab_row, tab_col) = usage_tab_text_position(&d, 32, 120, "Overview");

    assert_eq!(
        d.handle_click(tab_row, tab_col, 32, 120, None),
        DialogAction::Redraw
    );
    assert_eq!(d.usage_selected_tab(), Some(UsageDialogTab::Overview));
}

#[test]
fn usage_dialog_renders_deficit_and_runout_quota_labels() {
    let mut view = usage_view_fixture();
    view.buckets
        .push(jackin_protocol::control::QuotaBucketView {
            used_money: None,
            limit_money: None,
            severity: jackin_protocol::control::UsageSeverity::default(),
            label: "Weekly".to_owned(),
            used_label: Some("40% used".to_owned()),
            limit_label: Some("100%".to_owned()),
            remaining_percent: Some(60),
            reset_label: Some("Resets Jun 17 at 23:15".to_owned()),
            resets_at: None,
            status_slot: None,
            pace_label: Some("31% in deficit · Runs out in 21h 45m".to_owned()),
            status: jackin_protocol::control::UsageSnapshotStatus::Fresh,
        });
    let d = Dialog::new_usage(view);
    let state = d.usage_state().expect("usage state");
    let values: Vec<&str> = state
        .rows()
        .iter()
        .map(crate::tui::components::container_info_surface::ContainerInfoRow::value)
        .collect();

    assert!(values.iter().any(|value| {
        value.contains("60% left")
            && value.contains("31% in deficit")
            && value.contains("Runs out in 21h 45m")
            && value.contains("Resets Jun 17 at 23:15")
    }));

    let snapshot = d.to_ratatui_snapshot(None);
    let rect = d.box_rect(40, 100);
    let backend = TestBackend::new(100, 40);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            crate::tui::components::dialog_widgets::render_dialog_ratatui(frame, rect, &snapshot);
        })
        .unwrap();

    let buf = terminal.backend().buffer();
    let rendered = (0..40)
        .map(|y| (0..100).map(|x| buf[(x, y)].symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(rendered.contains("Weekly"), "{rendered}");
    assert!(rendered.contains("31% in deficit"), "{rendered}");
    assert!(rendered.contains("Runs out in 21h 45m"), "{rendered}");
    assert!(rendered.contains("Lasts until reset"), "{rendered}");
}

#[test]
fn usage_dialog_renders_dynamic_provider_quota_bucket_meters() {
    let mut view = usage_view_fixture();
    view.buckets = vec![
        jackin_protocol::control::QuotaBucketView {
            used_money: None,
            limit_money: None,
            severity: jackin_protocol::control::UsageSeverity::default(),
            label: "Tokens".to_owned(),
            used_label: Some("400M".to_owned()),
            limit_label: Some("1B".to_owned()),
            remaining_percent: Some(60),
            reset_label: Some("Resets Jun 17 at 23:15".to_owned()),
            resets_at: None,
            status_slot: None,
            pace_label: Some("31% in deficit · Runs out in 21h 45m".to_owned()),
            status: jackin_protocol::control::UsageSnapshotStatus::Fresh,
        },
        jackin_protocol::control::QuotaBucketView {
            used_money: None,
            limit_money: None,
            severity: jackin_protocol::control::UsageSeverity::default(),
            label: "MCP".to_owned(),
            used_label: Some("2h".to_owned()),
            limit_label: Some("5h".to_owned()),
            remaining_percent: Some(60),
            reset_label: Some("Resets 18:00".to_owned()),
            resets_at: None,
            status_slot: None,
            pace_label: Some("5 hours window".to_owned()),
            status: jackin_protocol::control::UsageSnapshotStatus::Fresh,
        },
        jackin_protocol::control::QuotaBucketView {
            used_money: None,
            limit_money: None,
            severity: jackin_protocol::control::UsageSeverity::default(),
            label: "Amp Free".to_owned(),
            used_label: Some("$12.00".to_owned()),
            limit_label: Some("$25.00".to_owned()),
            remaining_percent: Some(52),
            reset_label: None,
            resets_at: None,
            status_slot: None,
            pace_label: Some("replenishes +$1.00/hour".to_owned()),
            status: jackin_protocol::control::UsageSnapshotStatus::Fresh,
        },
        jackin_protocol::control::QuotaBucketView {
            used_money: None,
            limit_money: None,
            severity: jackin_protocol::control::UsageSeverity::default(),
            label: "MiniMax M1 Coding plan".to_owned(),
            used_label: Some("12K".to_owned()),
            limit_label: Some("100K".to_owned()),
            remaining_percent: Some(88),
            reset_label: Some("Resets tomorrow, 02:00".to_owned()),
            resets_at: None,
            status_slot: None,
            pace_label: Some("Coding plan".to_owned()),
            status: jackin_protocol::control::UsageSnapshotStatus::Fresh,
        },
    ];

    let d = Dialog::new_usage(view);
    let snapshot = d.to_ratatui_snapshot(None);
    let rect = d.box_rect(40, 120);
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            crate::tui::components::dialog_widgets::render_dialog_ratatui(frame, rect, &snapshot);
        })
        .unwrap();

    let buf = terminal.backend().buffer();
    let rendered = (0..40)
        .map(|y| (0..120).map(|x| buf[(x, y)].symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(rendered.contains("Tokens"), "{rendered}");
    assert!(rendered.contains("60% left"), "{rendered}");
    assert!(rendered.contains("31% in deficit"), "{rendered}");
    assert!(rendered.contains("MCP"), "{rendered}");
    assert!(rendered.contains("5 hours window"), "{rendered}");
    assert!(rendered.contains("Amp Free"), "{rendered}");
    assert!(rendered.contains("replenishes +$1.00/hour"), "{rendered}");
    assert!(rendered.contains("MiniMax M1 Coding plan"), "{rendered}");
    assert!(rendered.contains("88% left"), "{rendered}");
    assert!(rendered.contains("████"), "{rendered}");
    assert!(
        rendered
            .lines()
            .any(|line| line.chars().filter(|ch| matches!(*ch, '█' | '·')).count() >= 70),
        "quota meters must span the available dialog width: {rendered}"
    );
}

#[test]
fn usage_dialog_renders_extra_usage_monthly_cap() {
    let mut view = usage_view_fixture();
    view.buckets
        .push(jackin_protocol::control::QuotaBucketView {
            used_money: None,
            limit_money: None,
            severity: jackin_protocol::control::UsageSeverity::default(),
            label: "Extra usage".to_owned(),
            used_label: Some("SGD 78.49".to_owned()),
            limit_label: Some("SGD 260.00".to_owned()),
            remaining_percent: Some(70),
            reset_label: None,
            resets_at: None,
            status_slot: Some(jackin_protocol::control::StatusSlot::Spend),
            pace_label: None,
            status: jackin_protocol::control::UsageSnapshotStatus::Fresh,
        });
    let d = Dialog::new_usage(view);
    let state = d.usage_state().expect("usage state");
    let values: Vec<&str> = state
        .rows()
        .iter()
        .map(crate::tui::components::container_info_surface::ContainerInfoRow::value)
        .collect();

    assert!(values.iter().any(|value| {
        value.contains("30% used") && value.contains("Monthly cap: SGD 78.49 / SGD 260.00")
    }));

    let snapshot = d.to_ratatui_snapshot(None);
    let rect = d.box_rect(40, 100);
    let backend = TestBackend::new(100, 40);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            crate::tui::components::dialog_widgets::render_dialog_ratatui(frame, rect, &snapshot);
        })
        .unwrap();

    let buf = terminal.backend().buffer();
    let rendered = (0..40)
        .map(|y| (0..100).map(|x| buf[(x, y)].symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(rendered.contains("Extra usage"), "{rendered}");
    assert!(rendered.contains("30% used"), "{rendered}");
    assert!(
        rendered.contains("Monthly cap: SGD 78.49 / SGD 260.00"),
        "{rendered}"
    );
    let monthly = rendered
        .find("Monthly cap: SGD 78.49 / SGD 260.00")
        .expect("monthly cap");
    let used = rendered.find("30% used").expect("used percent");
    assert!(used < monthly, "{rendered}");
}

#[test]
fn usage_dialog_renders_dollar_budget_window() {
    let mut view = usage_view_fixture();
    view.buckets
        .push(jackin_protocol::control::QuotaBucketView {
            used_money: Some(jackin_protocol::control::Money::new(0, "USD", 2)),
            limit_money: Some(jackin_protocol::control::Money::new(2_500_000, "USD", 2)),
            severity: jackin_protocol::control::UsageSeverity::default(),
            label: "Amber Ladder".to_owned(),
            used_label: Some("$0.00 spent".to_owned()),
            limit_label: Some("$25,000.00".to_owned()),
            remaining_percent: Some(100),
            reset_label: Some("Resets in 66d".to_owned()),
            resets_at: Some(1_788_000_000),
            status_slot: None,
            pace_label: Some("0% used".to_owned()),
            status: jackin_protocol::control::UsageSnapshotStatus::Fresh,
        });
    let d = Dialog::new_usage(view);
    let state = d.usage_state().expect("usage state");
    let values: Vec<&str> = state
        .rows()
        .iter()
        .map(crate::tui::components::container_info_surface::ContainerInfoRow::value)
        .collect();
    assert!(
        values
            .iter()
            .any(|value| value.contains("Budget: $0.00 spent / $25,000.00")),
        "dollar-window cap must render: {values:?}"
    );
}

#[test]
fn usage_dialog_overview_tab_renders_cross_provider_summary() {
    let d = Dialog::new_usage_with_tab(usage_view_fixture(), UsageDialogTab::Overview);
    let state = d.usage_state().expect("usage state");
    let values: Vec<&str> = state
        .rows()
        .iter()
        .map(crate::tui::components::container_info_surface::ContainerInfoRow::value)
        .collect();
    let rows_debug = format!("{:?}", state.rows());

    assert!(!rows_debug.contains("Focused agent"));
    assert!(!rows_debug.contains("Focused account"));
    assert!(rows_debug.contains("OpenAI"));
    assert!(rows_debug.contains("Anthropic"));
    assert!(rows_debug.contains("xAI"));
    assert!(rows_debug.contains("Z.AI"));
    assert!(values.contains(&"37% left · Resets in 1h 21m (Jun 17, 23:15)"));
    assert!(values.contains(&"16% left · Resets in 46m (Jun 17, 22:40)"));
    assert!(values.contains(&"unsupported"));
    assert!(!rows_debug.contains("fresh · provider"));
    assert!(!rows_debug.contains("stale · provider"));

    let snapshot = d.to_ratatui_snapshot(None);
    let rect = d.box_rect(32, 100);
    let backend = TestBackend::new(100, 32);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            crate::tui::components::dialog_widgets::render_dialog_ratatui(frame, rect, &snapshot);
        })
        .unwrap();

    let buf = terminal.backend().buffer();
    let rendered = (0..32)
        .map(|y| (0..100).map(|x| buf[(x, y)].symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(rendered.contains("OpenAI      37% left"), "{rendered}");
    assert!(rendered.contains("Anthropic   16% left"), "{rendered}");
    assert!(rendered.contains("Resets in 1h 21m"), "{rendered}");
    assert!(rendered.contains("(Jun 17, 23:15)"), "{rendered}");
    assert!(rendered.contains("xAI        needs login"), "{rendered}");
    assert!(!rendered.contains("alexey@example.com"), "{rendered}");
    assert!(!rendered.contains("Pro 20x"), "{rendered}");
    assert!(!rendered.contains("fresh"), "{rendered}");
    assert!(rendered.contains("unsupported"), "{rendered}");
}

#[test]
fn usage_dialog_renders_amp_individual_credits_as_credits_section() {
    let d = Dialog::new_usage(amp_usage_view_fixture());
    let snapshot = d.to_ratatui_snapshot(None);
    let rect = d.box_rect(32, 100);
    let backend = TestBackend::new(100, 32);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            crate::tui::components::dialog_widgets::render_dialog_ratatui(frame, rect, &snapshot);
        })
        .unwrap();

    let buf = terminal.backend().buffer();
    let rendered = (0..32)
        .map(|y| (0..100).map(|x| buf[(x, y)].symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(rendered.contains("Amp"), "{rendered}");
    assert!(rendered.contains("account@personal.test"), "{rendered}");
    assert!(rendered.contains("Amp Free"), "{rendered}");
    assert!(rendered.contains("4% left"), "{rendered}");
    assert!(rendered.contains("Resets in 22h 40m"), "{rendered}");
    assert!(rendered.contains("Credits"), "{rendered}");
    assert!(rendered.contains("Individual credits: $4.76"), "{rendered}");
    assert!(
        !rendered.contains("Individual credits  remaining"),
        "{rendered}"
    );
}
