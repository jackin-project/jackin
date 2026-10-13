// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn amp_usage_view_fixture() -> jackin_protocol::control::FocusedUsageView {
    provider_usage_view_fixture(
        "Amp",
        "Amp",
        "account@personal.test",
        Some("Amp Free"),
        "Updated now",
        vec![
            jackin_protocol::control::QuotaBucketView {
                used_money: None,
                limit_money: None,
                severity: jackin_protocol::control::UsageSeverity::default(),
                label: "Amp Free".to_owned(),
                used_label: Some("$9.60".to_owned()),
                limit_label: Some("$10".to_owned()),
                remaining_percent: Some(4),
                reset_label: Some("Resets in 22h 40m".to_owned()),
                resets_at: None,
                status_slot: None,
                pace_label: None,
                status: jackin_protocol::control::UsageSnapshotStatus::Fresh,
            },
            jackin_protocol::control::QuotaBucketView {
                used_money: None,
                limit_money: None,
                severity: jackin_protocol::control::UsageSeverity::default(),
                label: "Individual credits".to_owned(),
                used_label: None,
                limit_label: Some("$4.76".to_owned()),
                remaining_percent: None,
                reset_label: None,
                resets_at: None,
                status_slot: None,
                pace_label: Some("Individual credits: $4.76".to_owned()),
                status: jackin_protocol::control::UsageSnapshotStatus::Fresh,
            },
        ],
    )
}

pub(super) fn xai_usage_view_fixture() -> jackin_protocol::control::FocusedUsageView {
    provider_usage_view_fixture(
        "Grok Build",
        "xAI",
        "account@work.test",
        Some("SuperGrok"),
        "Updated 4m ago",
        vec![quota_bucket(
            "Weekly",
            18,
            Some("Resets Jul 1 at 07:00"),
            None,
        )],
    )
}

pub(super) fn zai_usage_view_fixture() -> jackin_protocol::control::FocusedUsageView {
    provider_usage_view_fixture(
        "GLM / Z.AI",
        "Z.AI",
        "account@work.test",
        None,
        "Updated 2m ago",
        vec![
            quota_bucket("Tokens", 99, Some("Resets Jun 27 at 15:27"), None),
            quota_bucket(
                "MCP",
                100,
                Some("Resets Jul 13 at 15:27"),
                Some("0 / 100 (100 remaining)"),
            ),
            quota_bucket("5-hour", 100, Some("Resets 5 hours window"), None),
        ],
    )
}

pub(super) fn kimi_usage_view_fixture() -> jackin_protocol::control::FocusedUsageView {
    provider_usage_view_fixture(
        "Kimi",
        "Kimi",
        "account@work.test",
        None,
        "Updated 4m ago",
        vec![
            quota_bucket("Weekly", 100, Some("Resets Jul 1 at 15:17"), None),
            quota_bucket(
                "Rate Limit",
                100,
                Some("Resets 17:17"),
                Some("86% in reserve"),
            ),
        ],
    )
}

pub(super) fn minimax_usage_view_fixture() -> jackin_protocol::control::FocusedUsageView {
    provider_usage_view_fixture(
        "MiniMax",
        "MiniMax",
        "account@work.test",
        None,
        "Updated 3m ago",
        vec![
            quota_bucket(
                "General · 5h",
                100,
                Some("Resets 28m"),
                Some("Usage: 0 / 100"),
            ),
            quota_bucket(
                "General · Weekly",
                99,
                Some("Resets 4d"),
                Some("Usage: 1 / 100"),
            ),
            quota_bucket("Video", 100, Some("Resets 14h"), Some("Usage: 0 / 100")),
        ],
    )
}

pub(super) fn antigravity_usage_view_fixture() -> jackin_protocol::control::FocusedUsageView {
    provider_usage_view_fixture(
        "Antigravity",
        "Antigravity",
        "pilot@example.test",
        Some("Antigravity Pro"),
        "Updated 5m ago",
        vec![
            quota_bucket("Gemini · 5h", 73, Some("Resets in 1h 30m"), Some("On pace")),
            // Legacy weekly fallback: no percent, no meter. The label head
            // collides with the Gemini provider name, which used to route
            // this row through the overview arm (S4/S5 parity).
            text_bucket("Gemini · Weekly", "No data"),
            quota_bucket(
                "Other models · 5h",
                12,
                Some("Resets in 1h 30m"),
                Some("5% in deficit"),
            ),
        ],
    )
}

pub(super) fn render_usage_dialog_snapshot(width: u16, height: u16, tab: UsageDialogTab) -> String {
    render_usage_dialog_snapshot_for_view(width, height, tab, usage_view_fixture())
}

pub(super) fn render_usage_dialog_snapshot_for_view(
    width: u16,
    height: u16,
    tab: UsageDialogTab,
    view: jackin_protocol::control::FocusedUsageView,
) -> String {
    let d = Dialog::new_usage_with_tab(view, tab);
    let snapshot = d.to_ratatui_snapshot(None);
    let rect = d.box_rect(height, width);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            crate::tui::components::dialog_widgets::render_dialog_ratatui(frame, rect, &snapshot);
        })
        .unwrap();

    let buf = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn usage_tab_text_position(
    d: &Dialog,
    height: u16,
    width: u16,
    label: &str,
) -> (u16, u16) {
    let snapshot = d.to_ratatui_snapshot(None);
    let rect = d.box_rect(height, width);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            crate::tui::components::dialog_widgets::render_dialog_ratatui(frame, rect, &snapshot);
        })
        .unwrap();

    let buf = terminal.backend().buffer();
    for y in 0..height {
        let line = (0..width).map(|x| buf[(x, y)].symbol()).collect::<String>();
        if let Some(x) = line.find(label) {
            return (y, u16::try_from(x).expect("tab label column fits u16"));
        }
    }
    panic!("usage tab label {label:?} not rendered");
}

pub(super) fn s8_usage_scroll(d: &Dialog) -> (u16, u16) {
    let Dialog::Usage { scroll, .. } = d else {
        panic!("usage dialog");
    };
    (scroll.scroll_x, scroll.scroll_y)
}

pub(super) fn s8_usage_tab_bar_focused(d: &Dialog) -> bool {
    let Dialog::Usage {
        tab_bar_focused, ..
    } = d
    else {
        panic!("usage dialog");
    };
    *tab_bar_focused
}
