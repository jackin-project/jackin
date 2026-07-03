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

// ── truncate_spans_to_cols ────────────────────────────────────────────────────

#[test]
fn truncate_spans_empty_returns_empty() {
    assert!(truncate_spans_to_cols(&[], 80).is_empty());
}

#[test]
fn truncate_spans_all_fit() {
    let spans: &[jackin_tui::HintSpan<'_>] = &[
        jackin_tui::HintSpan::Key("A"),
        jackin_tui::HintSpan::Text("action"),
        jackin_tui::HintSpan::GroupSep,
        jackin_tui::HintSpan::Key("B"),
        jackin_tui::HintSpan::Text("other"),
    ];
    let result = truncate_spans_to_cols(spans, 80);
    assert_eq!(result.len(), spans.len(), "all spans must fit in 80 cols");
}

#[test]
fn truncate_spans_first_group_too_wide_returns_empty() {
    // A single span wider than max_cols -> nothing rendered.
    let spans: &[jackin_tui::HintSpan<'_>] = &[jackin_tui::HintSpan::Key(
        "a very long key that exceeds the narrow terminal",
    )];
    let result = truncate_spans_to_cols(spans, 5);
    assert!(
        result.is_empty(),
        "first-group overflow must return empty slice"
    );
}

#[test]
fn truncate_spans_keeps_fitting_groups_drops_overflowing() {
    // Three groups; limit set so only groups 1+2 fit.
    let spans: &[jackin_tui::HintSpan<'_>] = &[
        jackin_tui::HintSpan::Key("A"),
        jackin_tui::HintSpan::Text("short"),
        jackin_tui::HintSpan::GroupSep,
        jackin_tui::HintSpan::Key("B"),
        jackin_tui::HintSpan::Text("short"),
        jackin_tui::HintSpan::GroupSep,
        jackin_tui::HintSpan::Key("C"),
        jackin_tui::HintSpan::Text("overflows-the-budget-clearly"),
    ];
    let two_groups = jackin_tui::hint_row_cols(&spans[..5]); // A short GroupSep B short
    let result = truncate_spans_to_cols(spans, two_groups + 2);
    // Should keep groups 1+2 (spans[0..5]); trailing GroupSep is stripped.
    assert!(
        result.len() <= 5,
        "overflow group must be dropped: got {} spans",
        result.len()
    );
    assert!(
        !matches!(result.last(), Some(jackin_tui::HintSpan::GroupSep)),
        "trailing GroupSep must be stripped"
    );
}

#[test]
fn dynamic_key_hint_uses_key_style() {
    let area = Rect::new(0, 0, 40, 4);
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
        .find(|x| buf[(*x, 1)].symbol() == "C")
        .expect("key rendered");
    assert_eq!(buf[(key_cell, 1)].fg, color(jackin_tui::WHITE));
    assert!(
        buf[(key_cell, 1)]
            .style()
            .add_modifier
            .contains(Modifier::BOLD)
    );
}

#[test]
fn separator_hint_uses_shared_border_gray() {
    let area = Rect::new(0, 0, 40, 4);
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
        .find(|x| buf[(*x, 1)].symbol() == "·")
        .expect("separator rendered");
    assert_eq!(buf[(sep_cell, 1)].fg, jackin_tui::theme::BORDER_GRAY);
}
