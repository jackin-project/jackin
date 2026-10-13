// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `terminal`.
use super::terminal_area_from_size;
use ratatui::layout::Rect;

#[test]
fn terminal_area_from_size_uses_size_or_empty_fallback() {
    assert_eq!(
        terminal_area_from_size(Some((100, 30))),
        Rect::new(0, 0, 100, 30)
    );
    assert_eq!(terminal_area_from_size(None), Rect::default());
}
