// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn click_outside_returns_dismiss() {
    let rect = Rect::new(10, 5, 20, 10);
    assert_eq!(classify_click(rect, 5, 5), ModalClickResult::OutsideDismiss);
    assert_eq!(
        classify_click(rect, 35, 5),
        ModalClickResult::OutsideDismiss
    );
    assert_eq!(
        classify_click(rect, 15, 20),
        ModalClickResult::OutsideDismiss
    );
}

#[test]
fn click_inside_returns_hit() {
    let rect = Rect::new(10, 5, 20, 10);
    assert_eq!(classify_click(rect, 15, 8), ModalClickResult::InsideHit);
    assert_eq!(classify_click(rect, 10, 5), ModalClickResult::InsideHit);
    assert_eq!(classify_click(rect, 29, 14), ModalClickResult::InsideHit);
}
