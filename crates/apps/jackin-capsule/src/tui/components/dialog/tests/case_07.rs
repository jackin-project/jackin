// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn s8_usage_extreme_scroll_still_renders_chrome() {
    let mut d = Dialog::new_usage(usage_view_fixture());
    assert_eq!(d.handle_key(b"\t", None), DialogAction::Redraw);
    for _ in 0..500 {
        assert_eq!(d.handle_key(b"j", None), DialogAction::Redraw);
    }
    assert!(s8_usage_scroll(&d).1 >= 500);
    // Render clamps the runaway offset: chrome survives, no panic.
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
}
