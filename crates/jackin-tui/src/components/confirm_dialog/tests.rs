// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `confirm_dialog`.
use super::*;
use crate::components::ButtonFocus;
use crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::NONE,
    }
}

#[test]
fn y_commits_true() {
    let mut s = ConfirmState::new("Delete?");
    assert!(matches!(
        s.handle_key(key(KeyCode::Char('y'))),
        ModalOutcome::Commit(true)
    ));
}

#[test]
fn uppercase_y_commits_true() {
    let mut s = ConfirmState::new("Delete?");
    assert!(matches!(
        s.handle_key(key(KeyCode::Char('Y'))),
        ModalOutcome::Commit(true)
    ));
}

#[test]
fn n_commits_false() {
    let mut s = ConfirmState::new("Delete?");
    assert!(matches!(
        s.handle_key(key(KeyCode::Char('n'))),
        ModalOutcome::Commit(false)
    ));
}

#[test]
fn esc_cancels() {
    let mut s = ConfirmState::new("Delete?");
    assert!(matches!(
        s.handle_key(key(KeyCode::Esc)),
        ModalOutcome::Cancel
    ));
}

#[test]
fn arrow_is_noop() {
    let mut s = ConfirmState::new("Delete?");
    assert!(matches!(
        s.handle_key(key(KeyCode::Down)),
        ModalOutcome::Continue
    ));
}

#[test]
fn default_focus_is_no() {
    let s = ConfirmState::new("Delete?");
    assert_eq!(s.focus, ConfirmFocus::No);
}

#[test]
fn plain_exit_confirmation_focuses_yes_without_changing_destructive_default() {
    let exit = exit_confirm_state();
    let delete = ConfirmState::new("Delete?");

    assert_eq!(exit.focus, ConfirmFocus::Yes);
    assert_eq!(delete.focus, ConfirmFocus::No);
}

#[test]
fn tab_cycles_focus() {
    let mut s = ConfirmState::new("Delete?");
    assert_eq!(s.focus, ConfirmFocus::No);
    s.handle_key(key(KeyCode::Tab));
    assert_eq!(s.focus, ConfirmFocus::Yes);
    s.handle_key(key(KeyCode::Tab));
    assert_eq!(s.focus, ConfirmFocus::No);
}

#[test]
fn confirm_focus_ring_and_index_match_button_order() {
    assert_eq!(ConfirmFocus::Yes.index(), 0);
    assert_eq!(ConfirmFocus::No.index(), 1);
    assert_eq!(ConfirmFocus::Yes.next(), ConfirmFocus::No);
    assert_eq!(ConfirmFocus::No.next(), ConfirmFocus::Yes);
    assert_eq!(ConfirmFocus::Yes.prev(), ConfirmFocus::No);
    assert_eq!(ConfirmFocus::No.prev(), ConfirmFocus::Yes);
}

#[test]
fn confirm_focus_keys_keep_existing_toggle_semantics() {
    for code in [
        KeyCode::Tab,
        KeyCode::BackTab,
        KeyCode::Left,
        KeyCode::Right,
        KeyCode::Char('h'),
        KeyCode::Char('l'),
    ] {
        let mut state = ConfirmState::new("Delete?");
        assert_eq!(state.focus, ConfirmFocus::No);
        assert!(matches!(
            state.handle_key(key(code)),
            ModalOutcome::Continue
        ));
        assert_eq!(
            state.focus,
            ConfirmFocus::Yes,
            "{code:?} should toggle focus"
        );
    }
}

#[test]
fn enter_commits_focused_option() {
    let mut s = ConfirmState::new("Delete?");
    assert!(matches!(
        s.handle_key(key(KeyCode::Enter)),
        ModalOutcome::Commit(false)
    ));

    let mut s = ConfirmState::new("Delete?");
    s.handle_key(key(KeyCode::Tab));
    assert!(matches!(
        s.handle_key(key(KeyCode::Enter)),
        ModalOutcome::Commit(true)
    ));
}

#[test]
fn y_still_works_regardless_of_focus() {
    let mut s = ConfirmState::new("Delete?");
    assert!(matches!(
        s.handle_key(key(KeyCode::Char('y'))),
        ModalOutcome::Commit(true)
    ));
}

#[test]
fn details_prompt_renders_readable_source_details() {
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};

    let s = ConfirmState::details(
        "Review source",
        "Use this source?",
        vec![
            ("Name".into(), "primary".into()),
            ("Location".into(), "https://example.com/source.git".into()),
        ],
        vec![
            "External content may run commands.".into(),
            "Review the source before continuing.".into(),
        ],
    );
    let area = Rect::new(0, 0, 100, required_height(&s));
    let backend = TestBackend::new(area.width, area.height);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| render_confirm_dialog(f, area, &s)).unwrap();

    let buf = term.backend().buffer();
    let mut rendered = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            rendered.push_str(buf[(x, y)].symbol());
        }
        rendered.push('\n');
    }

    assert!(rendered.contains("Review source"));
    assert!(rendered.contains("Name: primary"));
    assert!(rendered.contains("Location: https://example.com/source.git"));
    assert!(rendered.contains("External content may run commands."));
    assert!(rendered.contains("Review the source before continuing."));
}

#[test]
fn confirm_button_hit_matches_data_loss_rendered_buttons() {
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};

    let s = exit_confirm_state_with_data_loss();
    let area = Rect::new(0, 0, 80, required_height(&s));
    let backend = TestBackend::new(area.width, area.height);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| render_confirm_dialog(f, area, &s)).unwrap();
    let buf = term.backend().buffer();

    let find = |needle: &str| {
        for y in 0..area.height {
            for x in 0..area.width {
                if buf[(x, y)].symbol() == needle {
                    return (x, y);
                }
            }
        }
        panic!("missing rendered button cell {needle:?}");
    };

    let yes = find("Y");
    let no = find("N");

    assert_eq!(confirm_button_hit(area, &s, yes.0, yes.1), Some(true));
    assert_eq!(confirm_button_hit(area, &s, no.0, no.1), Some(false));
    assert_eq!(confirm_button_hit(area, &s, area.x, area.y), None);
}

#[test]
fn default_dialog_has_symmetric_vertical_padding() {
    // The canonical dialog layout has exactly 1 leading spacer (row 1, after the top border)
    // and 1 trailing spacer (last inner row, before the bottom border). Verify that neither
    // the prompt nor the button row touches the top or bottom border.
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};

    let s = ConfirmState::new("Delete workspace?");
    let height = required_height(&s);
    let area = Rect::new(0, 0, 40, height);
    let backend = TestBackend::new(area.width, area.height);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| render_confirm_dialog(f, area, &s)).unwrap();
    let buf = term.backend().buffer();

    let row_content = |y: u16| {
        (0..area.width)
            .map(|x| buf[(x, y)].symbol().to_owned())
            .collect::<String>()
    };

    // Row 0 is the top border — must not contain prompt text.
    assert!(
        !row_content(0).contains("Delete"),
        "prompt must not be on the top border row"
    );
    // Last row (height-1) is the bottom border — must not contain button text.
    assert!(
        !row_content(height - 1).contains("Yes"),
        "buttons must not be on the bottom border row"
    );
    // Row 1 (first inner row) is the leading spacer — must be blank inside the border.
    let leading = row_content(1);
    // Strip the first and last characters (border glyphs, possibly multi-byte).
    let leading_inner: String = leading
        .chars()
        .skip(1)
        .take(leading.chars().count() - 2)
        .collect();
    assert!(
        leading_inner.trim().is_empty(),
        "row 1 must be the leading spacer (blank): {leading_inner:?}"
    );
    // Last inner row (height-2) is the trailing spacer — must be blank inside the border.
    let trailing = row_content(height - 2);
    let trailing_inner: String = trailing
        .chars()
        .skip(1)
        .take(trailing.chars().count() - 2)
        .collect();
    assert!(
        trailing_inner.trim().is_empty(),
        "last inner row must be the trailing spacer (blank): {trailing_inner:?}"
    );
}

#[test]
fn default_dialog_renders_followup_lines_as_dim_explanation() {
    use ratatui::{Terminal, backend::TestBackend, layout::Rect, style::Modifier};

    let s = ConfirmState::new("Purge instance?\nRemoves recovery state.");
    let height = required_height(&s);
    let area = Rect::new(0, 0, 60, height);
    let backend = TestBackend::new(area.width, area.height);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| render_confirm_dialog(f, area, &s)).unwrap();
    let buf = term.backend().buffer();

    let first_symbol_cell = |row: u16, needle: &str| {
        (0..area.width)
            .map(|x| (x, buf[(x, row)].clone()))
            .find(|(_, cell)| cell.symbol() == needle)
            .unwrap_or_else(|| panic!("missing {needle:?} on row {row}"))
    };

    let (_, question_cell) = first_symbol_cell(2, "P");
    assert_eq!(question_cell.fg, crate::theme::WHITE);
    assert!(question_cell.modifier.contains(Modifier::BOLD));

    let (_, explanation_cell) = first_symbol_cell(3, "R");
    assert_eq!(explanation_cell.fg, crate::theme::PHOSPHOR_DIM);
    assert!(!explanation_cell.modifier.contains(Modifier::BOLD));
}
