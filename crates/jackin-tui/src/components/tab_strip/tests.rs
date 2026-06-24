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

#[test]
fn tab_strip_exposes_rendered_cells_for_hit_testing() {
    let labels = [("General", true), ("Mounts", false)];
    let cells = TabStrip::new(&labels).cells(8);
    let expected = lay_out_tabs(&labels, 8);

    assert_eq!(cells.len(), expected.len());
    assert_eq!(cells[0].label, expected[0].label);
    assert_eq!(cells[0].active, expected[0].active);
    assert_eq!(cells[0].start_col, expected[0].start_col);
    assert_eq!(cells[0].cell_cols, expected[0].cell_cols);
    assert_eq!(cells[1].label, expected[1].label);
    assert_eq!(cells[1].active, expected[1].active);
    assert_eq!(cells[1].start_col, expected[1].start_col);
    assert_eq!(cells[1].cell_cols, expected[1].cell_cols);
}
