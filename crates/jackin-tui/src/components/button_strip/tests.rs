// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `button_strip`.
use super::{ButtonStripItem, button_strip_line};

#[test]
fn button_strip_pads_and_separates_labels() {
    let items = [ButtonStripItem::new("Save"), ButtonStripItem::new("Cancel")];

    let line = button_strip_line(&items, 0, "    ");

    let text: String = line
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect();
    // Each label gets 2-space padding on each side; gap is 4 spaces.
    // "  Save  " + "    " + "  Cancel  " = 2+4+2+4+2+6+2 chars.
    assert_eq!(text, "  Save        Cancel  ");
}
