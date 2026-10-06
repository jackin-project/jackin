// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{ConsoleFocusTarget, TabFocus};

#[test]
fn tab_focus_switches_between_tabs_and_content() {
    let mut focus = TabFocus::tab_bar("editor");
    assert!(focus.is_tab_bar());
    assert!(!focus.show_cursor_for(&"editor"));

    focus.focus_content("settings");
    assert_eq!(focus.focused(), ConsoleFocusTarget::Content("settings"));
    assert!(focus.show_cursor_for(&"settings"));

    focus.focus_tab_bar();
    assert!(focus.is_tab_bar());
}

#[test]
fn tab_focus_falls_back_to_tab_bar_and_tracks_one_content_identity() {
    let mut focus = TabFocus::tab_bar("editor");
    assert!(focus.is_tab_bar());
    assert!(focus.focused_content().is_none());

    focus.focus_content("settings");
    assert_eq!(focus.focused(), ConsoleFocusTarget::Content("settings"));
    assert_eq!(focus.focused_content(), Some("settings"));
    assert!(focus.is_content("settings"));
    assert!(focus.show_cursor_for(&"settings"));
    // Only one content identity is registered at a time (register-per-mutation).
    assert!(!focus.is_content("editor"));

    focus.focus_tab_bar();
    assert!(focus.is_tab_bar());
    assert!(!focus.show_cursor_for(&"settings"));
}
