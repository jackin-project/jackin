//! Tests for `pane`.
use super::*;
use ratatui::{Terminal, backend::TestBackend};

#[test]
fn pane_widget_renders_text_into_buffer() {
    let mut parser = vt100::Parser::new(5, 20, 100);
    parser.process(b"hello world");
    let screen = parser.screen().clone();

    let backend = TestBackend::new(20, 5);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            frame.render_widget(PaneBodyWidget::new(&screen), frame.area());
        })
        .unwrap();

    let buf = terminal.backend().buffer();
    let row0: String = (0..20).map(|x| buf[(x, 0)].symbol().to_string()).collect();
    assert!(
        row0.starts_with("hello world"),
        "expected text in buffer: {row0:?}"
    );
}

#[test]
fn pane_widget_maps_color_reset() {
    let color = vt100_color(vt100::Color::Default);
    assert_eq!(color, Color::Reset);
}

#[test]
fn pane_widget_maps_indexed_color() {
    let color = vt100_color(vt100::Color::Idx(196));
    assert_eq!(color, Color::Indexed(196));
}

#[test]
fn pane_widget_maps_rgb_color() {
    let color = vt100_color(vt100::Color::Rgb(0, 255, 65));
    assert_eq!(color, Color::Rgb(0, 255, 65));
}
