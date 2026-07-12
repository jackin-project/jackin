//! Tests for `pane`.
use super::*;
use crate::tui::socket_backend::term_color;
use jackin_term::DamageGrid;
use ratatui::{
    Terminal,
    backend::TestBackend,
    buffer::{Buffer, CellDiffOption},
    layout::Rect,
    style::{Color, Modifier},
};
use std::num::NonZeroU16;

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
fn pane_widget_renders_borrowed_view_into_buffer() {
    let mut grid = DamageGrid::new(5, 20, 100);
    grid.process(b"hello borrowed");
    let view = grid.scrollback_view(0, 5);

    let backend = TestBackend::new(20, 5);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            frame.render_widget(PaneBodyWidget::view(&view), frame.area());
        })
        .unwrap();

    let buf = terminal.backend().buffer();
    let row0: String = (0..20).map(|x| buf[(x, 0)].symbol().to_owned()).collect();
    assert!(
        row0.starts_with("hello borrowed"),
        "expected borrowed view text in buffer: {row0:?}"
    );
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

#[test]
fn pane_widget_forces_model_wide_lead_width() {
    for text in [
        "界",
        "ｶ\u{ff9e}",
        "☑\u{fe0f}",
        "👨\u{200d}👩\u{200d}👧\u{200d}👦",
    ] {
        let mut grid = DamageGrid::new(3, 10, 100);
        grid.process(format!("{text}Z").as_bytes());
        let snap = grid.dump();

        let backend = TestBackend::new(10, 3);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(PaneBodyWidget::new(&snap), frame.area());
            })
            .unwrap();

        let buf = terminal.backend().buffer();
        assert_eq!(buf[(0, 0)].symbol(), text);
        assert_eq!(
            buf[(0, 0)].diff_option,
            CellDiffOption::ForcedWidth(NonZeroU16::new(2).unwrap()),
            "{text:?} must use model-forced width"
        );
        assert_eq!(buf[(1, 0)].symbol(), " ");
        assert_eq!(buf[(1, 0)].diff_option, CellDiffOption::None);
        #[allow(deprecated, reason = "documented residual allow; prefer expect when site is lint-true")]
        {
            assert!(
                !buf[(1, 0)].skip,
                "ordinary continuation cell must not use legacy skip"
            );
        }
        assert_eq!(buf[(2, 0)].symbol(), "Z");
        assert_eq!(buf[(2, 0)].diff_option, CellDiffOption::None);
    }
}

#[test]
fn pane_widget_does_not_force_single_width_classes() {
    for text in ["e\u{301}", "·", "ｶ"] {
        let mut grid = DamageGrid::new(3, 10, 100);
        grid.process(text.as_bytes());
        let snap = grid.dump();

        let backend = TestBackend::new(10, 3);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(PaneBodyWidget::new(&snap), frame.area());
            })
            .unwrap();

        let buf = terminal.backend().buffer();
        assert_eq!(buf[(0, 0)].symbol(), text);
        assert_eq!(
            buf[(0, 0)].diff_option,
            CellDiffOption::None,
            "{text:?} must stay ordinary width"
        );
    }
}

#[test]
fn pane_widget_maps_extended_visible_sgr_modifiers() {
    let mut grid = DamageGrid::new(3, 10, 100);
    grid.process(b"\x1b[9;5;6;8mA");
    let snap = grid.dump();

    let backend = TestBackend::new(10, 3);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            frame.render_widget(PaneBodyWidget::new(&snap), frame.area());
        })
        .unwrap();

    let modifier = terminal.backend().buffer()[(0, 0)].modifier;
    assert!(modifier.contains(Modifier::CROSSED_OUT));
    assert!(modifier.contains(Modifier::SLOW_BLINK));
    assert!(modifier.contains(Modifier::RAPID_BLINK));
    assert!(modifier.contains(Modifier::HIDDEN));
}

#[test]
fn pane_widget_resets_forced_width_after_narrow_overwrite() {
    let backend = TestBackend::new(10, 3);
    let mut terminal = Terminal::new(backend).unwrap();

    let mut wide = DamageGrid::new(3, 10, 100);
    wide.process("界".as_bytes());
    let wide_snap = wide.dump();
    terminal
        .draw(|frame| {
            frame.render_widget(PaneBodyWidget::new(&wide_snap), frame.area());
        })
        .unwrap();
    assert!(matches!(
        terminal.backend().buffer()[(0, 0)].diff_option,
        CellDiffOption::ForcedWidth(_)
    ));

    let mut narrow = DamageGrid::new(3, 10, 100);
    narrow.process(b"x");
    let narrow_snap = narrow.dump();
    terminal
        .draw(|frame| {
            frame.render_widget(PaneBodyWidget::new(&narrow_snap), frame.area());
        })
        .unwrap();

    let buf = terminal.backend().buffer();
    assert_eq!(buf[(0, 0)].symbol(), "x");
    assert_eq!(buf[(0, 0)].diff_option, CellDiffOption::None);
    assert_eq!(buf[(1, 0)].symbol(), " ");
    assert_eq!(buf[(1, 0)].diff_option, CellDiffOption::None);
}

#[test]
#[should_panic(expected = "forced-width tail")]
fn pane_buffer_property_rejects_visible_forced_width_tail() {
    let area = Rect::new(0, 0, 4, 1);
    let mut buf = Buffer::empty(area);
    buf[(0, 0)]
        .set_symbol("界")
        .set_diff_option(CellDiffOption::ForcedWidth(NonZeroU16::new(2).unwrap()));
    buf[(1, 0)].set_symbol("x");

    debug_assert_pane_area_well_formed(area, &buf);
}

#[test]
#[should_panic(expected = "must not use CellDiffOption::Skip")]
fn pane_buffer_property_rejects_skip_cells() {
    let area = Rect::new(0, 0, 4, 1);
    let mut buf = Buffer::empty(area);
    buf[(0, 0)].set_diff_option(CellDiffOption::Skip);

    debug_assert_pane_area_well_formed(area, &buf);
}
