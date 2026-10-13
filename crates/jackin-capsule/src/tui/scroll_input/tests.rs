// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn raw_down_key_increments_vertical_scroll() {
    let mut scroll = DialogScroll::default();
    let axes = ScrollAxes {
        vertical: true,
        horizontal: false,
    };
    assert!(apply_raw_dialog_scroll_key(&mut scroll, b"\x1b[B", axes));
    assert!(scroll.scroll_y > 0);
    assert_eq!(scroll.scroll_x, 0);
}

#[test]
fn raw_key_ignored_when_axis_disabled() {
    let mut scroll = DialogScroll::default();
    let axes = ScrollAxes {
        vertical: false,
        horizontal: false,
    };
    assert!(!apply_raw_dialog_scroll_key(&mut scroll, b"\x1b[B", axes));
    assert_eq!(scroll.scroll_y, 0);
}

#[test]
fn sgr_wheel_down_increments_vertical_scroll() {
    let mut scroll = DialogScroll::default();
    let axes = ScrollAxes {
        vertical: true,
        horizontal: false,
    };
    // bit0 set means forward, which the product adapter decodes as ScrollDown.
    assert!(apply_sgr_wheel_button(&mut scroll, 0b0001, axes));
    assert!(scroll.scroll_y > 0);
}
