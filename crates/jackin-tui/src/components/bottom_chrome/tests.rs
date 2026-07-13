// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn bottom_chrome_reserves_hint_spacer_footer_rows() {
    let area = Rect::new(2, 4, 80, 12);
    let rows = bottom_chrome_areas(area);

    assert_eq!(rows.body, Rect::new(2, 4, 80, 9));
    assert_eq!(rows.hint, Rect::new(2, 13, 80, 1));
    assert_eq!(rows.spacer, Rect::new(2, 14, 80, 1));
    assert_eq!(rows.footer, Rect::new(2, 15, 80, 1));
}

#[test]
fn bottom_chrome_collapses_rows_that_do_not_fit() {
    let area = Rect::new(0, 0, 20, 2);
    let rows = bottom_chrome_areas(area);

    assert_eq!(rows.body.height, 0);
    assert_eq!(rows.hint.height, 0);
    assert_eq!(rows.spacer, Rect::new(0, 0, 20, 1));
    assert_eq!(rows.footer, Rect::new(0, 1, 20, 1));
}
