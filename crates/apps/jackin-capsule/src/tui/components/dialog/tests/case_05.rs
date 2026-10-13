// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn usage_dialog_right_arrow_switches_to_next_provider() {
    let mut d = Dialog::new_usage(usage_view_fixture());

    assert_eq!(
        d.handle_key(b"\x1b[C", None),
        DialogAction::SwitchUsageProvider {
            provider_label: "Claude".to_owned(),
            account_id: "test-tab-claude".to_owned(),
        }
    );
}

#[test]
fn usage_dialog_tab_key_moves_focus_to_content() {
    let mut d = Dialog::new_usage(usage_view_fixture());

    assert_eq!(d.handle_key(b"\t", None), DialogAction::Redraw);
    let Dialog::Usage {
        tab_bar_focused, ..
    } = d
    else {
        panic!("usage dialog");
    };
    assert!(!tab_bar_focused);
}

#[test]
fn usage_dialog_left_arrow_from_first_provider_switches_to_overview() {
    let mut d = Dialog::new_usage(usage_view_fixture());

    assert_eq!(d.handle_key(b"\x1b[D", None), DialogAction::Redraw);
    let state = d.usage_state().expect("usage state");
    assert_eq!(state.rows()[0].label(), "OpenAI");
    assert_eq!(
        state.rows()[0].value(),
        "37% left · Resets in 1h 21m (Jun 17, 23:15)"
    );
}

#[test]
fn usage_dialog_renders_inside_narrow_terminal() {
    let d = Dialog::new_usage(usage_view_fixture());
    let snapshot = d.to_ratatui_snapshot(None);
    let rect = d.box_rect(18, 60);
    let backend = TestBackend::new(60, 18);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            crate::tui::components::dialog_widgets::render_dialog_ratatui(frame, rect, &snapshot);
        })
        .unwrap();

    let buf = terminal.backend().buffer();
    let rendered = (0..18)
        .map(|y| (0..60).map(|x| buf[(x, y)].symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(rendered.contains("Usage"), "{rendered}");
    assert!(rendered.contains("OpenAI"), "{rendered}");
    assert!(!rendered.contains("OpenAI / Codex"), "{rendered}");
    assert!(rendered.contains("alexey@example.com"), "{rendered}");
    assert!(rendered.contains("Pro 20x"), "{rendered}");
    assert!(rendered.contains("Updated now"), "{rendered}");
    assert!(!rendered.contains("Account availability"), "{rendered}");
    assert!(!rendered.contains("2 buckets"), "{rendered}");
    assert!(!rendered.contains("Overview  Codex"), "{rendered}");
    assert!(!rendered.contains("████"), "{rendered}");
    assert!(rendered.contains("Session  37% left"), "{rendered}");
    assert!(!rendered.contains("Focused :"), "{rendered}");
}

#[test]
fn usage_dialog_stays_above_bottom_chrome_on_default_terminal() {
    let d = Dialog::new_usage_with_tab(zai_usage_view_fixture(), UsageDialogTab::Provider);
    let (row, _, height, _) = d.box_rect(24, 80);
    let content_bottom = crate::tui::components::status_bar::STATUS_BAR_ROWS
        + crate::tui::layout::available_content_rows(24);

    assert!(
        row + height <= content_bottom,
        "usage dialog must not overlap hint/footer chrome: row={row} height={height} content_bottom={content_bottom}"
    );
}

#[test]
fn snapshot_usage_dialog_narrow_60x18() {
    insta::assert_snapshot!(
        "usage_dialog_narrow_60x18",
        render_usage_dialog_snapshot(60, 18, UsageDialogTab::Provider)
    );
}

#[test]
fn snapshot_usage_dialog_medium_100x32_overview() {
    insta::assert_snapshot!(
        "usage_dialog_medium_100x32_overview",
        render_usage_dialog_snapshot(100, 32, UsageDialogTab::Overview)
    );
}

#[test]
fn snapshot_usage_dialog_wide_120x40() {
    insta::assert_snapshot!(
        "usage_dialog_wide_120x40",
        render_usage_dialog_snapshot(120, 40, UsageDialogTab::Provider)
    );
}

#[test]
fn snapshot_usage_dialog_openai_provider_120x48() {
    insta::assert_snapshot!(
        "usage_dialog_openai_provider_120x48",
        render_usage_dialog_snapshot_for_view(
            120,
            48,
            UsageDialogTab::Provider,
            openai_usage_view_fixture()
        )
    );
}

#[test]
fn snapshot_usage_dialog_anthropic_provider_120x42() {
    insta::assert_snapshot!(
        "usage_dialog_anthropic_provider_120x42",
        render_usage_dialog_snapshot_for_view(
            120,
            42,
            UsageDialogTab::Provider,
            anthropic_usage_view_fixture()
        )
    );
}

#[test]
fn snapshot_usage_dialog_amp_wide_100x32() {
    insta::assert_snapshot!(
        "usage_dialog_amp_wide_100x32",
        render_usage_dialog_snapshot_for_view(
            100,
            32,
            UsageDialogTab::Provider,
            amp_usage_view_fixture()
        )
    );
}

#[test]
fn snapshot_usage_dialog_xai_provider_100x28() {
    insta::assert_snapshot!(
        "usage_dialog_xai_provider_100x28",
        render_usage_dialog_snapshot_for_view(
            100,
            28,
            UsageDialogTab::Provider,
            xai_usage_view_fixture()
        )
    );
}

#[test]
fn snapshot_usage_dialog_zai_provider_100x34() {
    insta::assert_snapshot!(
        "usage_dialog_zai_provider_100x34",
        render_usage_dialog_snapshot_for_view(
            100,
            34,
            UsageDialogTab::Provider,
            zai_usage_view_fixture()
        )
    );
}

#[test]
fn snapshot_usage_dialog_kimi_provider_100x30() {
    insta::assert_snapshot!(
        "usage_dialog_kimi_provider_100x30",
        render_usage_dialog_snapshot_for_view(
            100,
            30,
            UsageDialogTab::Provider,
            kimi_usage_view_fixture()
        )
    );
}

#[test]
fn snapshot_usage_dialog_minimax_provider_100x32() {
    insta::assert_snapshot!(
        "usage_dialog_minimax_provider_100x32",
        render_usage_dialog_snapshot_for_view(
            100,
            32,
            UsageDialogTab::Provider,
            minimax_usage_view_fixture()
        )
    );
}

#[test]
fn usage_dialog_geometry_counts_rendered_section_lines() {
    let mut view = usage_view_fixture();
    view.buckets.extend([
        jackin_protocol::control::QuotaBucketView {
            used_money: None,
            limit_money: None,
            severity: jackin_protocol::control::UsageSeverity::default(),
            label: "Tokens".to_owned(),
            used_label: Some("100K".to_owned()),
            limit_label: Some("1M".to_owned()),
            remaining_percent: Some(90),
            reset_label: Some("Resets Jun 17 at 14:00".to_owned()),
            resets_at: None,
            status_slot: None,
            pace_label: Some("20% in reserve".to_owned()),
            status: jackin_protocol::control::UsageSnapshotStatus::Fresh,
        },
        jackin_protocol::control::QuotaBucketView {
            used_money: None,
            limit_money: None,
            severity: jackin_protocol::control::UsageSeverity::default(),
            label: "MCP".to_owned(),
            used_label: Some("2".to_owned()),
            limit_label: Some("100".to_owned()),
            remaining_percent: Some(98),
            reset_label: Some("Resets Jul 1 at 14:00".to_owned()),
            resets_at: None,
            status_slot: None,
            pace_label: Some("2 / 100 (98 remaining)".to_owned()),
            status: jackin_protocol::control::UsageSnapshotStatus::Fresh,
        },
    ]);
    let d = Dialog::new_usage(view);
    let state = d.usage_state().expect("usage state");
    let usage_height = crate::tui::components::dialog_widgets::usage_info_required_height(&state);

    assert!(usage_height >= 7);
    assert_eq!(d.box_rect(50, 120).2, usage_height);
    // Bug 2: the scroll bound now uses the same width-wrapped line set and body
    // viewport (box − border − tab strip) the renderer uses. Assert overflow at a
    // wide-but-short terminal (≥64 cols → wide layout matching `usage_height`;
    // few rows → the box clamps below the content) so the dialog scrolls. A tall
    // terminal must NOT advertise vertical scroll — the content fits its body.
    // Bug 2: the scroll bound now uses the same width-wrapped line set and body
    // viewport (box − border − tab strip) the renderer uses. At a wide terminal
    // (≥64 cols → wide layout matching `usage_height`) the content overflows a
    // short box and scrolls, and fits — no vertical scroll — at a tall one.
    assert!(d.body_scroll_axes(18, 120, None).vertical);
    assert!(!d.body_scroll_axes(50, 120, None).vertical);
}

#[test]
fn container_info_esc_dismisses() {
    let mut d = container_info_fixture();
    assert_eq!(d.handle_key(b"\x1b", None), DialogAction::Dismiss);
}

#[test]
fn container_info_q_dismisses() {
    // ContainerInfo has no editable input, so `q` is also a valid
    // dismiss key (same as the list-style dialogs).
    let mut d = container_info_fixture();
    assert_eq!(d.handle_key(b"q", None), DialogAction::Dismiss);
}

#[test]
fn container_info_arrow_keys_are_redraw_noops() {
    // Read-only modal, no navigation. Arrow keys must neither
    // dismiss the dialog nor produce a Command-like action — a
    // bare Redraw keeps the box on screen and waits for Enter /
    // Esc.
    let mut d = container_info_fixture();
    assert_eq!(d.handle_key(b"\x1b[A", None), DialogAction::Redraw);
    assert_eq!(d.handle_key(b"\x1b[B", None), DialogAction::Redraw);
    assert_eq!(d.handle_key(b"\x1b[C", None), DialogAction::Redraw);
    assert_eq!(d.handle_key(b"\x1b[D", None), DialogAction::Redraw);
}

#[test]
fn container_info_left_and_right_keys_scroll_horizontally() {
    let mut d = container_info_fixture();

    assert_eq!(d.handle_key(b"\x1b[C", None), DialogAction::Redraw);
    let Dialog::ContainerInfo { scroll, .. } = &d else {
        unreachable!()
    };
    assert_eq!(scroll.scroll_x, 1);

    assert_eq!(d.handle_key(b"\x1b[D", None), DialogAction::Redraw);
    let Dialog::ContainerInfo { scroll, .. } = &d else {
        unreachable!()
    };
    assert_eq!(scroll.scroll_x, 0);
}

#[test]
fn container_info_clamp_body_scroll_reduces_overscroll() {
    let mut d = container_info_fixture();
    let Dialog::ContainerInfo { scroll, .. } = &mut d else {
        unreachable!()
    };
    scroll.scroll_x = u16::MAX;
    scroll.scroll_y = u16::MAX;

    d.clamp_body_scroll(40, 100, None);

    let Dialog::ContainerInfo { scroll, .. } = &d else {
        unreachable!()
    };
    assert_ne!(scroll.scroll_x, u16::MAX);
    assert_ne!(scroll.scroll_y, u16::MAX);
}

#[test]
fn github_context_clamp_body_scroll_reduces_overscroll() {
    let pr = pull_request_fixture();
    let view = github_view_for_fixture(&pr);
    let mut d = Dialog::GitHubContext {
        copied: false,
        scroll: {
            let mut __scroll = termrock::scroll::DialogScroll::default();
            __scroll.scroll_x = u16::MAX;
            __scroll.scroll_y = u16::MAX;
            __scroll
        },
    };

    d.clamp_body_scroll(12, 40, Some(&view));

    let Dialog::GitHubContext { scroll, .. } = &d else {
        unreachable!()
    };
    assert_ne!(scroll.scroll_x, u16::MAX);
    assert_ne!(scroll.scroll_y, u16::MAX);
}

#[test]
fn agent_picker_section_labels_are_bare_not_dash_padded() {
    // Defect 28 regression: section labels must be bare text ("agents", "shells")
    // not "── agents ──". render_separator adds the surrounding dashes; if the
    // label already contains them, the output doubles.
    let d = picker(vec!["claude"]);
    let snapshot = d.to_ratatui_snapshot(None);
    use crate::tui::components::dialog_widgets::{DialogRatatuiSnapshot, PickerItem};
    if let DialogRatatuiSnapshot::FilterPicker { items, .. } = snapshot {
        for item in &items {
            if let PickerItem::Section(label) = item {
                assert!(
                    !label.contains("──"),
                    "section label must be bare text, not dash-padded: {label:?}"
                );
                assert!(!label.is_empty(), "section label must not be empty");
            }
        }
    } else {
        panic!("expected FilterPicker snapshot");
    }
}
