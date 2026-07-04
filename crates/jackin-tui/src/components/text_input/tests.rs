// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use ratatui::{Terminal, backend::TestBackend, layout::Rect};

#[test]
fn labeled_text_input_dialog_renders_shared_shell_and_cursor() {
    let backend = TestBackend::new(40, 7);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            render_labeled_text_input_dialog(
                frame,
                Rect::new(2, 1, 30, 5),
                "Rename tab",
                "Name",
                "alpha",
                2,
            );
        })
        .unwrap();

    let rendered = format!("{:?}", terminal.backend().buffer());
    assert!(rendered.contains("Rename tab"));
    assert!(rendered.contains("Name:"));
    assert!(rendered.contains("alpha"));
    let buf = terminal.backend().buffer();
    let cursor_cell = (0..buf.area.height)
        .flat_map(|y| (0..buf.area.width).map(move |x| (x, y)))
        .find(|(x, y)| {
            let cell = &buf[(*x, *y)];
            cell.symbol() == "p" && cell.style().add_modifier.contains(Modifier::BOLD)
        });
    assert!(
        cursor_cell.is_some(),
        "cursor cell should use the shared bold inverse style"
    );
}

#[test]
fn text_input_prompt_rect_matches_launch_prompt_shape() {
    let area = Rect::new(0, 0, 120, 30);
    let rect = text_input_prompt_rect(area);
    assert_eq!(rect.width, 72);
    assert_eq!(rect.height, 5);
    assert_eq!(rect.x, 24);
    assert_eq!(rect.y, 12);
}
