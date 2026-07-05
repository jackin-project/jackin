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
    let tab_start = u16::try_from(jackin_tui::display_cols(" jackin❯ ")).unwrap() + 1;

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
            .saturating_add(u16::try_from(jackin_tui::display_cols(&cell.name)).unwrap())
            .saturating_add(2);
        (buf[(x, 0)].symbol().to_owned(), buf[(x, 0)].fg)
    };

    assert_eq!(
        glyph_cell(&plan.cells[0]),
        ("▶".to_owned(), jackin_tui::theme::DEBUG_AMBER)
    );
    assert_eq!(
        glyph_cell(&plan.cells[1]),
        ("◆".to_owned(), jackin_tui::theme::PHOSPHOR_GREEN)
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

// ── wrapped hint rows ─────────────────────────────────────────────────────────

fn row_text(buf: &Buffer, row: u16) -> String {
    (0..buf.area.width)
        .map(|x| buf[(x, row)].symbol().to_owned())
        .collect()
}

#[test]
fn narrow_hint_wraps_groups_instead_of_dropping_them() {
    let area = Rect::new(0, 0, 24, 5);
    let mut buf = Buffer::empty(area);
    render_hint_spans_row(
        &mut buf,
        area,
        &[
            jackin_tui::HintSpan::Key("A"),
            jackin_tui::HintSpan::Text("alpha"),
            jackin_tui::HintSpan::GroupSep,
            jackin_tui::HintSpan::Key("B"),
            jackin_tui::HintSpan::Text("bravo"),
            jackin_tui::HintSpan::GroupSep,
            jackin_tui::HintSpan::Key("C"),
            jackin_tui::HintSpan::Text("charlie"),
        ],
    );

    let rows = [row_text(&buf, 0), row_text(&buf, 1), row_text(&buf, 2)];
    let joined = rows.join("\n");
    assert!(
        joined.contains("A alpha"),
        "first group missing: {joined:?}"
    );
    assert!(
        joined.contains("B bravo"),
        "second group missing: {joined:?}"
    );
    assert!(
        joined.contains("C charlie"),
        "third group missing: {joined:?}"
    );
    assert!(
        rows.iter().filter(|row| !row.trim().is_empty()).count() >= 2,
        "narrow hints should wrap across multiple rows: {joined:?}"
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
            jackin_tui::HintSpan::DynKey("Ctrl-\\".to_owned()),
            jackin_tui::HintSpan::Text("menu"),
        ],
    );

    let key_cell = (0..area.width)
        .find(|x| buf[(*x, 0)].symbol() == "C")
        .expect("key rendered");
    assert_eq!(buf[(key_cell, 0)].fg, jackin_tui::theme::WHITE);
    assert!(
        buf[(key_cell, 0)]
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
            jackin_tui::HintSpan::Key("A"),
            jackin_tui::HintSpan::Text("alpha"),
            jackin_tui::HintSpan::Sep,
            jackin_tui::HintSpan::Key("B"),
            jackin_tui::HintSpan::Text("bravo"),
        ],
    );

    let sep_cell = (0..area.width)
        .find(|x| buf[(*x, 0)].symbol() == "·")
        .expect("separator rendered");
    assert_eq!(buf[(sep_cell, 0)].fg, jackin_tui::theme::BORDER_GRAY);
}
