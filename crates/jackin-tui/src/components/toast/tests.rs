// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use ratatui::{Terminal, backend::TestBackend, layout::Rect};

use super::*;

#[test]
fn toast_rect_anchors_to_top_right() {
    let rect = toast_rect(
        Rect::new(0, 0, 149, 39),
        Toast::new("Selection copied").top_margin(1),
    )
    .expect("toast should fit");

    assert_eq!(rect.height, 3);
    assert_eq!(rect.width, 20);
    assert_eq!(rect.x, 127);
    assert_eq!(rect.y, 1);
}

#[test]
fn render_toast_draws_message_and_border() {
    let backend = TestBackend::new(40, 8);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            render_toast(
                frame,
                frame.area(),
                Toast::new("Selection copied").top_margin(1),
            );
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let rendered = format!("{buffer:?}");
    assert!(rendered.contains("Selection copied"));
    assert_eq!(buffer[(18, 1)].fg, PHOSPHOR_GREEN);
}
