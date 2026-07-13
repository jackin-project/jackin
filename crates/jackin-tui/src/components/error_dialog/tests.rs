// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `error_dialog`.
use super::*;
use crossterm::event::{KeyCode, KeyEventKind, KeyEventState, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

#[test]
fn enter_dismisses() {
    let mut state = ErrorPopupState::new("Save failed", "workspace already exists");
    assert!(matches!(
        state.handle_key(key(KeyCode::Enter)),
        ModalOutcome::Cancel
    ));
}

#[test]
fn esc_dismisses() {
    let mut state = ErrorPopupState::new("Save failed", "workspace already exists");
    assert!(matches!(
        state.handle_key(key(KeyCode::Esc)),
        ModalOutcome::Cancel
    ));
}

#[test]
fn o_dismisses() {
    let mut state = ErrorPopupState::new("Save failed", "workspace already exists");
    assert!(matches!(
        state.handle_key(key(KeyCode::Char('o'))),
        ModalOutcome::Cancel
    ));
}

#[test]
fn unhandled_key_continues() {
    let mut state = ErrorPopupState::new("Save failed", "workspace already exists");
    assert!(matches!(
        state.handle_key(key(KeyCode::Char('x'))),
        ModalOutcome::Continue
    ));
}

#[test]
fn required_height_respects_caller_supplied_max() {
    let state = ErrorPopupState::new("Save failed", "word ".repeat(500));
    assert!(required_height(&state, 30, 15) <= 15);
    assert!(required_height(&state, 30, 1) >= 7);
}

#[test]
fn required_height_matches_five_slot_layout() {
    let state = ErrorPopupState::new(
        "Launch failed",
        "Derived image build failed while installing role dependencies.\nOpen diagnostics run jk-run-3d7e23 for the full log.",
    );
    let inner_width = 60;
    assert_eq!(
        required_height(&state, inner_width, 99),
        estimated_message_rows(&state, inner_width) + 6
    );
}

#[test]
fn render_single_line_message_is_visible() {
    let state = ErrorPopupState::new("Role not found", "repository not found");
    let area = Rect::new(0, 0, 60, required_height(&state, 56, 25));
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| render_error_dialog_in(frame, area, &state))
        .unwrap();

    let buffer = terminal.backend().buffer();
    let mut rendered = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            rendered.push_str(buffer[(x, y)].symbol());
        }
        rendered.push('\n');
    }
    assert!(
        rendered.contains("repository not found"),
        "message should be visible in popup:\n{rendered}"
    );
}

#[test]
fn render_single_line_message_has_one_blank_row_before_ok() {
    let state = ErrorPopupState::new(
        "Load role failed",
        "Repository is not available, or you do not have access.",
    );
    let area = Rect::new(0, 0, 90, 10);
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| render_error_dialog_in(frame, area, &state))
        .unwrap();

    let buffer = terminal.backend().buffer();
    let row_string = |y| {
        (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect::<String>()
    };
    let message_y = (0..buffer.area.height)
        .find(|y| row_string(*y).contains("Repository is not available"))
        .expect("message row should render");
    let ok_y = (0..buffer.area.height)
        .find(|y| row_string(*y).contains("OK"))
        .expect("OK row should render");

    assert_eq!(
        ok_y,
        message_y + 2,
        "exactly one blank row should separate message and OK"
    );
    let spacer = row_string(message_y + 1);
    assert!(
        !spacer.contains("Repository") && !spacer.contains("OK"),
        "spacer row should be blank between message and OK: {spacer:?}"
    );
}

#[test]
fn render_helper_does_not_add_extra_blank_row_after_ok() {
    let state = ErrorPopupState::new(
        "Launch failed",
        "Derived image build failed while installing role dependencies.\nOpen diagnostics run jk-run-3d7e23 for the full log.",
    );
    let area = Rect::new(0, 0, 62, 20);
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| render_error_dialog(frame, area, &state))
        .unwrap();

    let buffer = terminal.backend().buffer();
    let row_string = |y| {
        (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect::<String>()
    };
    let ok_y = (0..buffer.area.height)
        .find(|y| row_string(*y).contains("OK"))
        .expect("OK row should render");
    let bottom_border_y = ((ok_y + 1)..buffer.area.height)
        .find(|y| row_string(*y).contains("└"))
        .expect("bottom border should render after OK");

    assert_eq!(
        bottom_border_y,
        ok_y + 2,
        "exactly one blank row should separate OK and bottom border"
    );
    let trailing = row_string(ok_y + 1);
    assert!(
        !trailing.contains("OK") && !trailing.contains("└"),
        "trailing spacer row should be blank inside the border: {trailing:?}"
    );
}

#[test]
fn rows_extend_estimated_height() {
    let base = ErrorPopupState::new("Launch failed", "build failed");
    let with_rows = ErrorPopupState::new("Launch failed", "build failed").with_rows(vec![
        ErrorPopupRow::new("Run", "jk-run-123"),
        ErrorPopupRow::new("Log", "/tmp/jackin/run.jsonl"),
    ]);

    assert_eq!(
        estimated_message_rows(&with_rows, 80),
        estimated_message_rows(&base, 80) + 2
    );
    assert_eq!(
        required_height(&with_rows, 80, 99),
        required_height(&base, 80, 99) + 2
    );
}

#[test]
fn row_value_rects_are_inside_inner_area_and_non_overlapping() {
    let state = ErrorPopupState::new("Launch failed", "build failed").with_rows(vec![
        ErrorPopupRow::new("Run", "jk-run-123"),
        ErrorPopupRow::new("Log", "/tmp/jackin/run.jsonl")
            .hyperlink("file:///tmp/jackin/run.jsonl"),
    ]);
    let inner = Rect::new(1, 1, 58, 8);
    let rects = state.row_value_rects(inner);

    assert_eq!(rects.len(), 2);
    assert!(rects.iter().all(|rect| rect.x >= inner.x));
    assert!(rects.iter().all(|rect| rect.right() <= inner.right()));
    assert!(rects.iter().all(|rect| rect.y >= inner.y));
    assert!(rects.iter().all(|rect| rect.bottom() <= inner.bottom()));
    assert_ne!(rects[0].y, rects[1].y);
}

#[test]
fn hyperlink_overlay_emits_osc8_for_link_rows() {
    let state = ErrorPopupState::new("Launch failed", "build failed").with_rows(vec![
        ErrorPopupRow::new("Run", "jk-run-123"),
        ErrorPopupRow::new("Log", "/tmp/jackin/run.jsonl")
            .hyperlink("file:///tmp/jackin/run.jsonl"),
    ]);
    let inner = Rect::new(1, 1, 58, 8);
    let overlay = String::from_utf8(hyperlink_overlay(inner, &state)).unwrap();

    assert!(overlay.contains("\u{1b}]8;;file:///tmp/jackin/run.jsonl\u{1b}\\"));
    assert!(overlay.contains("/tmp/jackin/run.jsonl"));
    assert!(overlay.contains("\u{1b}]8;;\u{1b}\\"));
    assert!(!overlay.contains("jk-run-123"));
}

#[test]
fn rendering_rows_places_label_text_at_expected_cells() {
    let state = ErrorPopupState::new("Launch failed", "build failed")
        .with_rows(vec![ErrorPopupRow::new("Run", "jk-run-123")]);
    let area = Rect::new(0, 0, 60, required_height(&state, 58, 20));
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| render_error_dialog_in(frame, area, &state))
        .unwrap();

    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(3, 3)].symbol(), "R");
    assert_eq!(buffer[(4, 3)].symbol(), "u");
    assert_eq!(buffer[(5, 3)].symbol(), "n");
    assert_eq!(buffer[(6, 3)].symbol(), ":");
}

#[test]
fn plain_popup_renders_danger_border_and_ok_chip() {
    let state = ErrorPopupState::new("Save failed", "workspace already exists");
    let area = Rect::new(0, 0, 60, required_height(&state, 58, 20));
    let backend = TestBackend::new(area.width, area.height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| render_error_dialog_in(frame, area, &state))
        .unwrap();

    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(0, 0)].fg, crate::theme::DANGER_RED);
    let ok_cell = (0..buffer.area.height)
        .flat_map(|y| (0..buffer.area.width).map(move |x| (x, y)))
        .find(|(x, y)| buffer[(*x, *y)].symbol() == "O")
        .expect("OK chip should render");
    assert_eq!(buffer[ok_cell].bg, WHITE);
}
