//! Tests for `tab_strip`.
use super::{TabStrip, tab_underline_line};
use crate::lay_out_tabs;

#[test]
fn underline_marks_only_active_tab_when_focused() {
    let cells = lay_out_tabs(&[("General", true), ("Mounts", false)], 0);

    let text: String = tab_underline_line(&cells, true)
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect();

    assert_eq!(text, "━━━━━━━━━          ");
}

#[test]
fn tab_strip_exposes_two_rows() {
    let labels = [("General", true), ("Mounts", false)];
    let backend = ratatui::backend::TestBackend::new(24, 2);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            TabStrip::new(&labels)
                .focused(true)
                .render(frame, frame.area());
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(0, 0)].symbol(), " ");
    assert_eq!(buffer[(0, 1)].symbol(), "━");
}
