// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::runtime::{SurfaceFocus, SurfaceFocusTarget};

#[test]
fn tab_bar_focus_reports_no_content() {
    let focus = SurfaceFocus::tab_bar(7u8);
    assert_eq!(focus.focused(), SurfaceFocusTarget::TabBar);
    assert_eq!(focus.focused_content(), None);
}

#[test]
fn content_focus_reports_selected_value() {
    let focus = SurfaceFocus::content(7u8);
    assert_eq!(focus.focused(), SurfaceFocusTarget::Content(7));
    assert_eq!(focus.focused_content(), Some(7));
}
