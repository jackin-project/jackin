//! Tests for `chrome`.
use super::*;
use crate::tui::components::status_bar::status_bar_plan;
use crate::tui::layout::Tab;
use ratatui::{Terminal, backend::TestBackend};

#[test]
fn status_bar_renders_without_tabs() {
    let backend = TestBackend::new(80, 2);
    let mut terminal = Terminal::new(backend).unwrap();
    let plan = status_bar_plan(80, &[], 0, &[], PrefixMode::Idle);
    terminal
        .draw(|frame| {
            frame.render_widget(
                StatusBarWidget {
                    plan: &plan,
                    prefix_mode: PrefixMode::Idle,
                    hovered_tab: None,
                    menu_hovered: false,
                },
                frame.area(),
            );
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    // Brand pill should appear in row 0
    let row0: String = (0..9).map(|x| buf[(x, 0)].symbol().to_owned()).collect();
    assert!(row0.contains("jackin'"), "brand pill missing: {row0:?}");
}

#[test]
fn status_bar_renders_shared_tab_underline() {
    let tabs = [
        Tab::new_single("shell", 1, "test"),
        Tab::new_single("agent", 2, "test"),
    ];
    let backend = TestBackend::new(80, 2);
    let mut terminal = Terminal::new(backend).unwrap();
    let plan = status_bar_plan(80, &tabs, 0, &[], PrefixMode::Idle);
    terminal
        .draw(|frame| {
            frame.render_widget(
                StatusBarWidget {
                    plan: &plan,
                    prefix_mode: PrefixMode::Idle,
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
fn status_bar_resets_canvas_across_unused_columns() {
    let tabs = [
        Tab::new_single("Claude", 1, "test"),
        Tab::new_single("Codex", 2, "test"),
    ];
    let backend = TestBackend::new(100, 2);
    let mut terminal = Terminal::new(backend).unwrap();
    let plan = status_bar_plan(100, &tabs, 1, &[], PrefixMode::Idle);
    terminal
        .draw(|frame| {
            frame.render_widget(
                StatusBarWidget {
                    plan: &plan,
                    prefix_mode: PrefixMode::Idle,
                    hovered_tab: None,
                    menu_hovered: false,
                },
                frame.area(),
            );
        })
        .unwrap();

    let buf = terminal.backend().buffer();
    let filler_x = 70;
    assert_eq!(buf[(filler_x, 0)].symbol(), " ");
    assert_eq!(
        buf[(filler_x, 0)].bg,
        Color::Reset,
        "unused status-strip cells must not inherit the brand-green background"
    );
    assert_eq!(buf[(filler_x, 1)].symbol(), " ");
    assert_eq!(buf[(filler_x, 1)].bg, Color::Reset);
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
    assert_eq!(buf[(0, 0)].fg, jackin_tui::theme::PHOSPHOR_GREEN);
    assert_eq!(buf[(2, 0)].fg, jackin_tui::theme::WHITE);
}

#[test]
fn unfocused_pane_border_uses_shared_panel_palette() {
    let backend = TestBackend::new(20, 10);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            frame.render_widget(
                PaneBorderWidget {
                    title: "shell".into(),
                    focused: false,
                },
                frame.area(),
            );
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    assert_eq!(buf[(0, 0)].fg, jackin_tui::theme::PHOSPHOR_DARK);
}
