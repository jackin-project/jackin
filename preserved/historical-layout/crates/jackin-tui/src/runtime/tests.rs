// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use ratatui::{Terminal, backend::TestBackend, widgets::Paragraph};

use super::{SurfaceFocus, SurfaceFocusTarget, UpdateResult, drive_render};

#[test]
fn update_result_merges_redraw_and_product_effects() {
    let update = UpdateResult::clean()
        .merge(UpdateResult::with_effect("open"))
        .merge(UpdateResult::with_effect("copy"));
    assert!(update.is_dirty());
    assert_eq!(update.effects(), &["open", "copy"]);
}

#[test]
fn drive_render_uses_the_product_terminal_once() {
    let mut terminal = Terminal::new(TestBackend::new(12, 2)).expect("test terminal");
    drive_render(&mut terminal, |frame| {
        frame.render_widget(Paragraph::new("jackin runtime"), frame.area());
    })
    .expect("draw product frame");
    let text: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect();
    assert!(text.contains("jackin runti"));
}

#[test]
fn surface_focus_switches_between_tabs_and_product_content() {
    let mut focus = SurfaceFocus::tab_bar("editor");
    assert!(focus.is_tab_bar());
    assert!(!focus.show_cursor_for(&"editor"));

    focus.focus_content("settings");
    assert_eq!(focus.focused(), SurfaceFocusTarget::Content("settings"));
    assert!(focus.show_cursor_for(&"settings"));

    focus.focus_tab_bar();
    assert!(focus.is_tab_bar());
}

#[test]
fn trparity_surface_focus_moves_between_tab_bar_and_content() {
    let mut focus = SurfaceFocus::tab_bar("editor");
    assert!(focus.is_tab_bar());
    assert!(focus.focused_content().is_none());

    focus.focus_content("settings");
    assert_eq!(focus.focused(), SurfaceFocusTarget::Content("settings"));
    assert_eq!(focus.focused_content(), Some("settings"));
    assert!(focus.is_content("settings"));
    assert!(focus.show_cursor_for(&"settings"));
    // Only one content identity is registered at a time (focus.rs register()).
    assert!(!focus.is_content("editor"));

    focus.focus_tab_bar();
    assert!(focus.is_tab_bar());
    assert!(!focus.show_cursor_for(&"settings"));
}
