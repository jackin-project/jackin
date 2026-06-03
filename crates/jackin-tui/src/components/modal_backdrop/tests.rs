//! Tests for `modal_backdrop`.
use super::*;
use ratatui::{Terminal, backend::TestBackend};

#[test]
fn modal_backdrop_fills_area_with_dialog_backdrop() {
    let backend = TestBackend::new(10, 5);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| frame.render_widget(ModalBackdrop, frame.area()))
        .unwrap();
    let buf = terminal.backend().buffer();
    let expected = crate::theme::color(crate::DIALOG_BACKDROP);
    assert_eq!(buf[(0, 0)].symbol(), " ");
    assert_eq!(buf[(0, 0)].bg, expected);
    assert_eq!(buf[(9, 4)].bg, expected);
}
