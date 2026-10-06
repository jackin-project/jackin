// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn chrome_frame(
    hover: Option<HoverTarget>,
    debug_run_id: Option<&str>,
    clipboard_image_notice: Option<&str>,
    link_hover_notice: Option<&str>,
) -> ratatui::buffer::Buffer {
    let tabs = [Tab::new_single("Codex", 1, "codex")];
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let status_plan =
        crate::tui::components::status_bar::status_bar_plan(80, &tabs, 0, &[], PrefixMode::Idle);
    terminal
        .draw(|frame| {
            render_capsule_ratatui_frame(
                frame,
                CapsuleRatatuiFrame {
                    tabs: &tabs,
                    status_plan: &status_plan,
                    term_cols: 80,
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
                    selection_copied: false,
                    scrollbars: &[],
                    branch: Some("main"),
                    usage_status_label: None,
                    pull_request: None,
                    pull_request_loading: false,
                    instance_id_label: "jk-test",
                    hover_target: hover,
                    scrollback_active: false,
                    main_scroll_axes: termrock::scroll::ScrollAxes::default(),
                    debug_run_id,
                    dialog_hint_spans: None,
                    palette_key: 0x1C,
                    clipboard_image_notice,
                    link_hover_notice,
                },
            );
        })
        .unwrap();
    terminal.backend().buffer().clone()
}

pub(super) fn chip_start_col(row: &str) -> u16 {
    let byte = row.find("jk-run-test").expect("chip start");
    u16::try_from(row[..byte].chars().count()).unwrap_or(u16::MAX)
}

pub(super) fn row_text(buf: &ratatui::buffer::Buffer, y: u16) -> String {
    (0..buf.area.width)
        .map(|x| buf[(x, y)].symbol().to_owned())
        .collect()
}
