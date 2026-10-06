// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn selection_copy_toast_keeps_status_and_bottom_chrome_rows_free() {
    let tabs = [Tab::new_single("Codex", 1, "codex")];
    let backend = TestBackend::new(90, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let status_plan =
        crate::tui::components::status_bar::status_bar_plan(90, &tabs, 0, &[], PrefixMode::Idle);

    terminal
        .draw(|frame| {
            render_capsule_ratatui_frame(
                frame,
                CapsuleRatatuiFrame {
                    tabs: &tabs,
                    status_plan: &status_plan,
                    term_cols: 90,
                    term_rows: 24,
                    panes: &[],
                    pane_titles: &[],
                    focus_owner: jackin_tui::runtime::SurfaceFocus::content(1),
                    zoomed: false,
                    dialog_open: false,
                    dialog_snapshot: None,
                    pane_screens: &[],
                    prefix_mode: PrefixMode::Idle,
                    hovered_tab: None,
                    menu_hovered: false,
                    selection: None,
                    selection_copied: true,
                    scrollbars: &[],
                    branch: None,
                    usage_status_label: None,
                    pull_request: None,
                    pull_request_loading: false,
                    instance_id_label: "jk-test",
                    hover_target: None,
                    scrollback_active: false,
                    main_scroll_axes: termrock::scroll::ScrollAxes::default(),
                    debug_run_id: None,
                    dialog_hint_spans: None,
                    palette_key: 0x1C,
                    clipboard_image_notice: None,
                    link_hover_notice: None,
                },
            );
        })
        .unwrap();

    let buf = terminal.backend().buffer();
    let row = |y: u16| -> String { (0..90).map(|x| buf[(x, y)].symbol().to_owned()).collect() };
    let all_rows: Vec<String> = (0..24).map(row).collect();
    assert!(
        all_rows.iter().any(|row| row.contains("Selection copied")),
        "selection copy toast should be visible: {all_rows:?}"
    );
    assert!(
        !all_rows[..usize::from(STATUS_BAR_ROWS)]
            .iter()
            .any(|row| row.contains("Selection copied")),
        "selection copy toast must not draw over status rows: {all_rows:?}"
    );
    let content_bottom = STATUS_BAR_ROWS + available_content_rows(24);
    assert!(
        !all_rows[usize::from(content_bottom)..]
            .iter()
            .any(|row| row.contains("Selection copied")),
        "selection copy toast must not draw over hint/spacer/footer rows: {all_rows:?}"
    );
    assert!(
        all_rows[0].contains("jackin❯"),
        "status brand missing: {:?}",
        all_rows[0]
    );
}

#[test]
fn spawn_failure_message_prefixes_visible_agent_label() {
    assert_eq!(
        spawn_failure_message("claude", "missing binary"),
        "claude: missing binary"
    );
    assert_eq!(spawn_failure_agent_label(Some("claude")), "claude");
    assert_eq!(spawn_failure_agent_label(None), "shell");
    assert_eq!(
        spawn_request_failure_message("codex", "missing binary"),
        "spawn codex failed: missing binary"
    );
}

#[test]
fn spawn_capacity_messages_report_visible_limits() {
    assert_eq!(
        tab_limit_failure_message(32),
        "tab limit reached (32); close one before spawning another"
    );
    assert_eq!(
        pane_limit_failure_message(64),
        "pane limit reached (64); close some panes before opening more"
    );
}
