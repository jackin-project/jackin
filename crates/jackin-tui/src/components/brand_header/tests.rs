// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `brand_header`.
use super::*;
use ratatui::{Terminal, backend::TestBackend};

#[test]
fn renders_brand_pill_and_label() {
    let backend = TestBackend::new(32, 1);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| frame.render_widget(BrandHeader::new("Console"), frame.area()))
        .unwrap();
    let buffer = terminal.backend().buffer();
    let row: String = (0..32)
        .map(|x| buffer[(x, 0)].symbol().to_owned())
        .collect();
    assert!(row.contains(" jackin❯  · Console"), "row: {row:?}");
    // Green block, black word, white chevron — never the same colour.
    assert_eq!(buffer[(1, 0)].bg, BRAND_BLOCK);
    assert_eq!(buffer[(1, 0)].fg, Color::Black);
    assert_eq!(buffer[(7, 0)].symbol(), "❯");
    assert_eq!(buffer[(7, 0)].fg, WHITE);
    assert_eq!(buffer[(11, 0)].fg, PHOSPHOR_DARK);
}
