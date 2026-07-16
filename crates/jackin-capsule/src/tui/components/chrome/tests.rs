// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `chrome`.
use super::*;
use crate::tui::components::status_bar::status_bar_plan;
use crate::tui::layout::Tab;
use crate::tui::model::VisibleAgentState;
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
                    focused: false,
                },
                frame.area(),
            );
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    // Brand pill should appear in row 0
    let row0: String = (0..9).map(|x| buf[(x, 0)].symbol().to_owned()).collect();
    assert!(row0.contains("jackin❯"), "brand pill missing: {row0:?}");
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
                    focused: false,
                },
                frame.area(),
            );
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    let tab_start = u16::try_from(termrock::text::display_cols(" jackin❯ ")).unwrap() + 1;

    assert_eq!(buf[(tab_start, 1)].symbol(), "━");
    assert_eq!(buf[(tab_start, 1)].fg, jackin_core::tui_theme::text_fg());
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
                    focused: false,
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
fn status_bar_renders_working_idle_done_and_unknown_glyphs() {
    let tabs = [
        Tab::new_single("Work", 1, "test"),
        Tab::new_single("Idle", 2, "test"),
        Tab::new_single("Done", 3, "test"),
        Tab::new_single("None", 4, "test"),
    ];
    let states = [
        (1, VisibleAgentState::Working),
        (2, VisibleAgentState::Idle),
        (3, VisibleAgentState::Done),
        (4, VisibleAgentState::Unknown),
    ];
    let backend = TestBackend::new(100, 2);
    let mut terminal = Terminal::new(backend).unwrap();
    let plan = status_bar_plan(100, &tabs, 0, &states, PrefixMode::Idle);
    terminal
        .draw(|frame| {
            frame.render_widget(
                StatusBarWidget {
                    plan: &plan,
                    prefix_mode: PrefixMode::Idle,
                    hovered_tab: None,
                    menu_hovered: false,
                    focused: false,
                },
                frame.area(),
            );
        })
        .unwrap();

    let buf = terminal.backend().buffer();
    let glyph_cell = |cell: &StatusTabCell| {
        let x = cell
            .start_col0
            .saturating_add(u16::try_from(termrock::text::display_cols(&cell.name)).unwrap())
            .saturating_add(2);
        (buf[(x, 0)].symbol().to_owned(), buf[(x, 0)].fg)
    };

    assert_eq!(
        glyph_cell(&plan.cells[0]),
        ("▶".to_owned(), jackin_core::tui_theme::DEBUG_AMBER)
    );
    assert_eq!(
        glyph_cell(&plan.cells[1]),
        ("◆".to_owned(), termrock::style::PHOSPHOR_GREEN)
    );
    assert_eq!(glyph_cell(&plan.cells[2]).0, "○");
    assert_eq!(glyph_cell(&plan.cells[3]).0, " ");
}

#[test]
fn dialog_backdrop_fills_with_black() {
    let backend = TestBackend::new(10, 5);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            frame.render_widget(termrock::widgets::Backdrop::default(), frame.area());
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    let expected = jackin_core::tui_theme::DIALOG_BACKDROP;
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
    assert_eq!(buf[(0, 0)].symbol(), "┌");
    assert_eq!(buf[(19, 0)].symbol(), "┐");
    assert_eq!(buf[(0, 9)].symbol(), "└");
    assert_eq!(buf[(19, 9)].symbol(), "┘");
    assert_eq!(buf[(0, 0)].fg, termrock::style::PHOSPHOR_GREEN);
    assert_eq!(buf[(2, 0)].fg, jackin_core::tui_theme::text_fg());
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
    assert_eq!(buf[(0, 0)].symbol(), "┌");
    assert_eq!(buf[(19, 0)].symbol(), "┐");
    assert_eq!(buf[(0, 9)].symbol(), "└");
    assert_eq!(buf[(19, 9)].symbol(), "┘");
    assert_eq!(
        buf[(0, 0)].fg,
        Theme::default()
            .style(termrock::style::Role::Border)
            .fg
            .expect("border role must define a foreground")
    );
}

// ── wrapped hint rows ─────────────────────────────────────────────────────────

fn row_text(buf: &Buffer, row: u16) -> String {
    (0..buf.area.width)
        .map(|x| buf[(x, row)].symbol().to_owned())
        .collect()
}

fn hint_row(area: Rect) -> u16 {
    area.height.saturating_sub(3)
}

#[test]
fn hint_row_sits_between_one_blank_row_above_and_below() {
    let area = Rect::new(0, 0, 24, 5);
    let mut buf = Buffer::empty(area);
    render_hint_spans_row(
        &mut buf,
        area,
        &[
            termrock::widgets::HintSpan::Key("A"),
            termrock::widgets::HintSpan::Text("alpha"),
            termrock::widgets::HintSpan::GroupSep,
            termrock::widgets::HintSpan::Key("B"),
            termrock::widgets::HintSpan::Text("bravo"),
            termrock::widgets::HintSpan::GroupSep,
            termrock::widgets::HintSpan::Key("C"),
            termrock::widgets::HintSpan::Text("charlie"),
        ],
    );

    let row_above = row_text(&buf, hint_row(area).saturating_sub(1));
    let row = row_text(&buf, hint_row(area));
    let row_below = row_text(&buf, hint_row(area).saturating_add(1));
    assert!(
        row_above.trim().is_empty(),
        "top spacer polluted: {row_above:?}"
    );
    assert!(
        row.contains("A alpha"),
        "first visible hint group missing: {row:?}"
    );
    assert!(
        row_below.trim().is_empty(),
        "bottom spacer polluted: {row_below:?}"
    );
}

#[test]
fn dynamic_key_hint_uses_key_style() {
    let area = Rect::new(0, 0, 40, 5);
    let mut buf = Buffer::empty(area);
    render_hint_spans_row(
        &mut buf,
        area,
        &[
            termrock::widgets::HintSpan::DynKey("Ctrl-\\".to_owned()),
            termrock::widgets::HintSpan::Text("menu"),
        ],
    );

    let y = hint_row(area);
    let key_cell = (0..area.width)
        .find(|x| buf[(*x, y)].symbol() == "C")
        .expect("key rendered");
    assert_eq!(buf[(key_cell, y)].fg, jackin_core::tui_theme::text_fg());
    assert!(
        buf[(key_cell, y)]
            .style()
            .add_modifier
            .contains(Modifier::BOLD)
    );
}

#[test]
fn separator_hint_uses_shared_border_gray() {
    let area = Rect::new(0, 0, 40, 5);
    let mut buf = Buffer::empty(area);
    render_hint_spans_row(
        &mut buf,
        area,
        &[
            termrock::widgets::HintSpan::Key("A"),
            termrock::widgets::HintSpan::Text("alpha"),
            termrock::widgets::HintSpan::Sep,
            termrock::widgets::HintSpan::Key("B"),
            termrock::widgets::HintSpan::Text("bravo"),
        ],
    );

    let y = hint_row(area);
    let sep_cell = (0..area.width)
        .find(|x| buf[(*x, y)].symbol() == "·")
        .expect("separator rendered");
    assert_eq!(buf[(sep_cell, y)].fg, jackin_core::tui_theme::border_fg());
}
