//! Tests for `pane`.
use super::*;
use jackin_term::DamageGrid;
use ratatui::{Terminal, backend::TestBackend};

#[test]
fn pane_widget_renders_text_into_buffer() {
    let mut grid = DamageGrid::new(5, 20, 100);
    grid.process(b"hello world");
    let snap = grid.dump();

    let backend = TestBackend::new(20, 5);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            frame.render_widget(PaneBodyWidget::new(&snap), frame.area());
        })
        .unwrap();

    let buf = terminal.backend().buffer();
    let row0: String = (0..20).map(|x| buf[(x, 0)].symbol().to_owned()).collect();
    assert!(
        row0.starts_with("hello world"),
        "expected text in buffer: {row0:?}"
    );
}

#[test]
fn pane_widget_patch_updates_only_dirty_rows() {
    let mut grid = DamageGrid::new(3, 20, 100);
    grid.process(b"\x1b[1;1Hfirst row\x1b[2;1Hsecond row");
    drop(grid.dump_dirty_patch());
    grid.process(b"\x1b[2;1Hchanged");
    let patch = grid.dump_dirty_patch();

    let backend = TestBackend::new(20, 3);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let area = frame.area();
            frame.buffer_mut()[(0, 0)].set_symbol("X");
            frame.buffer_mut()[(0, 1)].set_symbol("Y");
            frame.render_widget(PaneBodyWidget::from_patch(&patch), area);
        })
        .unwrap();

    let buf = terminal.backend().buffer();
    assert_eq!(buf[(0, 0)].symbol(), "X");
    assert_eq!(buf[(0, 1)].symbol(), "c");
}

#[test]
fn pane_widget_maps_color_reset() {
    let color = term_color(jackin_term::Color::Default);
    assert_eq!(color, Color::Reset);
}

#[test]
fn pane_widget_maps_indexed_color() {
    let color = term_color(jackin_term::Color::Idx(196));
    assert_eq!(color, Color::Indexed(196));
}

#[test]
fn pane_widget_maps_rgb_color() {
    let color = term_color(jackin_term::Color::Rgb(0, 255, 65));
    assert_eq!(color, Color::Rgb(0, 255, 65));
}
