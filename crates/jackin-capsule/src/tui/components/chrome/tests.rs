//! Tests for `chrome`.
use super::*;
use ratatui::{Terminal, backend::TestBackend};

#[test]
fn status_bar_renders_without_tabs() {
    let backend = TestBackend::new(80, 2);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            frame.render_widget(
                StatusBarWidget {
                    tabs: &[],
                    active_tab: 0,
                    cols: 80,
                    sessions_state: &[],
                    prefix_mode: crate::tui::components::status_bar::PrefixMode::Idle,
                    hovered_tab: None,
                    menu_hovered: false,
                },
                frame.area(),
            );
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    // Brand pill should appear in row 0
    let row0: String = (0..9).map(|x| buf[(x, 0)].symbol().to_string()).collect();
    assert!(row0.contains("jackin'"), "brand pill missing: {row0:?}");
}

#[test]
fn status_bar_renders_shared_tab_underline() {
    let tabs = [Tab::new_single("shell", 1), Tab::new_single("agent", 2)];
    let backend = TestBackend::new(80, 2);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            frame.render_widget(
                StatusBarWidget {
                    tabs: &tabs,
                    active_tab: 0,
                    cols: 80,
                    sessions_state: &[],
                    prefix_mode: crate::tui::components::status_bar::PrefixMode::Idle,
                    hovered_tab: None,
                    menu_hovered: false,
                },
                frame.area(),
            );
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    let tab_start = u16::try_from(jackin_tui::display_cols(BRAND_TEXT)).unwrap() + 1;

    assert_eq!(buf[(tab_start, 1)].symbol(), "━");
    assert_eq!(buf[(tab_start, 1)].fg, jackin_tui::theme::WHITE);
}

#[test]
fn dialog_backdrop_fills_with_black() {
    let backend = TestBackend::new(10, 5);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            frame.render_widget(DialogBackdrop, frame.area());
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    let expected = jackin_tui::theme::DIALOG_BACKDROP;
    assert_eq!(buf[(0, 0)].bg, expected);
    assert_eq!(buf[(9, 4)].bg, expected);
}

#[test]
fn pane_border_renders_border() {
    let backend = TestBackend::new(20, 10);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            frame.render_widget(
                PaneBorderWidget {
                    title: "shell".into(),
                    focused: true,
                },
                frame.area(),
            );
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    // Top-left corner should be a border character
    let tl = buf[(0, 0)].symbol();
    assert!(!tl.trim().is_empty(), "top-left border missing");
}
