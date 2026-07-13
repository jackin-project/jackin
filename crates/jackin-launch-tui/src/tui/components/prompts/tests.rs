// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

fn row_text(buf: &Buffer, row: u16, width: u16) -> String {
    (0..width)
        .map(|x| buf[(x, row)].symbol().to_owned())
        .collect()
}

#[test]
fn text_prompt_uses_shared_bottom_chrome_rows() {
    let area = Rect::new(0, 0, 80, 12);
    let input = TextInputState::new_allow_empty("Context7 API key", "");
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).expect("test backend should initialize");

    terminal
        .draw(|frame| draw_text_prompt(frame, &input, true))
        .expect("render should succeed");

    let hint = row_text(terminal.backend().buffer(), area.height - 3, area.width);
    let spacer = row_text(terminal.backend().buffer(), area.height - 2, area.width);
    let footer = row_text(terminal.backend().buffer(), area.height - 1, area.width);
    assert!(
        hint.contains("empty") && hint.contains("skip"),
        "prompt hint should render in the shared hint row: {hint:?}"
    );
    assert!(
        !spacer.contains("skip") && !spacer.contains("Context7"),
        "spacer row should stay separate from prompt hints: {spacer:?}"
    );
    assert!(
        !footer.contains("skip") && !footer.contains("Context7"),
        "footer row should remain reserved below prompt hints: {footer:?}"
    );
}
