// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{PickerRow, SelectList, SelectListState, render_picker_list};
use crate::theme::{PHOSPHOR_DARK, PHOSPHOR_GREEN};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::{ListItem, Widget};

fn row_symbols(buf: &Buffer, y: u16, width: u16) -> String {
    (0..width)
        .map(|x| buf[(x, y)].symbol().to_owned())
        .collect()
}

#[test]
fn separator_spans_full_width_and_centers_label() {
    let width = 24u16;
    let area = Rect {
        x: 0,
        y: 0,
        width,
        height: 4,
    };
    let mut buf = Buffer::empty(area);
    let rows = vec![
        PickerRow::Separator("agents".to_owned()),
        PickerRow::Item(ListItem::new("Claude")),
        PickerRow::Item(ListItem::new("Codex")),
    ];
    render_picker_list(area, &mut buf, rows, Some(1));

    let sep = row_symbols(&buf, 0, width);
    assert!(
        sep.starts_with('\u{2500}') && sep.ends_with('\u{2500}'),
        "divider must span full width: {sep:?}"
    );
    assert!(sep.contains("agents"), "label present: {sep:?}");

    let chars: Vec<char> = sep.chars().collect();
    let left = chars.iter().take_while(|c| **c == '\u{2500}').count();
    let right = chars.iter().rev().take_while(|c| **c == '\u{2500}').count();
    assert!(
        left.abs_diff(right) <= 1,
        "label not centered: left={left} right={right} in {sep:?}"
    );

    assert_eq!(buf[(0u16, 0u16)].fg, PHOSPHOR_DARK);
}

#[test]
fn item_rows_keep_selection_gutter() {
    let width = 20u16;
    let area = Rect {
        x: 0,
        y: 0,
        width,
        height: 3,
    };
    let mut buf = Buffer::empty(area);
    let rows = vec![
        PickerRow::Item(ListItem::new("Claude")),
        PickerRow::Item(ListItem::new("Codex")),
    ];
    render_picker_list(area, &mut buf, rows, Some(0));
    assert_eq!(buf[(0u16, 0u16)].symbol(), "\u{25b8}");
}

#[test]
fn rich_picker_lines_get_shared_selection_chrome() {
    let area = Rect {
        x: 0,
        y: 0,
        width: 24,
        height: 3,
    };
    let mut buf = Buffer::empty(area);
    let lines = vec![
        ratatui::text::Line::from(vec![
            ratatui::text::Span::raw("alpha"),
            ratatui::text::Span::raw("  "),
            ratatui::text::Span::raw("dim context"),
        ]),
        ratatui::text::Line::from("beta"),
    ];

    super::render_picker_lines(area, &mut buf, lines, Some(1));

    assert_eq!(buf[(0u16, 1u16)].symbol(), "\u{25b8}");
    assert_eq!(buf[(0u16, 1u16)].bg, PHOSPHOR_GREEN);
    assert_eq!(
        buf[(0u16, 0u16)].symbol(),
        " ",
        "callers pass raw content; shared renderer owns the cursor gutter"
    );
}

#[test]
fn select_list_right_left_keys_scroll_horizontally() {
    let mut state = SelectListState::new(vec!["0123456789abcdef".to_owned()]);

    let _ = state.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    assert_eq!(state.scroll_x(), 4);

    let _ = state.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    assert_eq!(state.scroll_x(), 0);
}

#[test]
fn select_list_renders_horizontal_scroll_window() {
    let mut state = SelectListState::new(vec!["0123456789abcdef".to_owned()]);
    let _ = state.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));

    let area = Rect {
        x: 0,
        y: 0,
        width: 12,
        height: 6,
    };
    let mut buf = Buffer::empty(area);
    SelectList::new(&state, "pick").render(area, &mut buf);

    let rendered = row_symbols(&buf, 3, 12);
    assert!(
        rendered.contains("456789"),
        "scrolled row should expose shifted label window: {rendered:?}"
    );
    let scrollbar = row_symbols(&buf, 4, 12);
    assert!(
        scrollbar.contains('━'),
        "wide labels should render horizontal scrollbar: {scrollbar:?}"
    );
}
