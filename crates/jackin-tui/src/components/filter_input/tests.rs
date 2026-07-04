// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `filter_input`.
use super::*;
use ratatui::{Terminal, backend::TestBackend};

#[test]
fn empty_filter_shows_placeholder() {
    let backend = TestBackend::new(32, 1);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| frame.render_widget(FilterInput::new(""), frame.area()))
        .unwrap();
    let row: String = (0..32)
        .map(|x| terminal.backend().buffer()[(x, 0)].symbol().to_owned())
        .collect();
    assert!(row.contains("Filter: ░░░░░░░░░░░░░░░░░░░░"));
}

#[test]
fn populated_filter_shows_cursor() {
    let line = filter_input_line("abc");
    let joined: String = line
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect();
    assert_eq!(joined, "Filter: abc█");
    assert!(
        line.spans[2]
            .style
            .add_modifier
            .contains(Modifier::SLOW_BLINK)
    );
}
