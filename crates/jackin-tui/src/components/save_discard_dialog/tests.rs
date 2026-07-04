// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `save_discard_dialog`.
use super::*;
use crossterm::event::{KeyCode, KeyEventKind, KeyEventState, KeyModifiers};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

#[test]
fn default_focus_is_cancel() {
    let s = SaveDiscardState::new("?");
    assert_eq!(s.focus, SaveDiscardFocus::Cancel);
}

#[test]
fn shortcuts_commit_or_cancel() {
    let mut s = SaveDiscardState::new("?");
    assert!(matches!(
        s.handle_key(key(KeyCode::Char('s'))),
        ModalOutcome::Commit(SaveDiscardChoice::Save)
    ));
    let mut s = SaveDiscardState::new("?");
    assert!(matches!(
        s.handle_key(key(KeyCode::Char('d'))),
        ModalOutcome::Commit(SaveDiscardChoice::Discard)
    ));
    let mut s = SaveDiscardState::new("?");
    assert!(matches!(
        s.handle_key(key(KeyCode::Esc)),
        ModalOutcome::Cancel
    ));
}

#[test]
fn enter_commits_focused_button() {
    let mut s = SaveDiscardState::new("?");
    drop(s.handle_key(key(KeyCode::Tab)));
    assert!(matches!(
        s.handle_key(key(KeyCode::Enter)),
        ModalOutcome::Commit(SaveDiscardChoice::Save)
    ));

    let mut s = SaveDiscardState::new("?");
    assert!(matches!(
        s.handle_key(key(KeyCode::Enter)),
        ModalOutcome::Cancel
    ));
}

#[test]
fn save_discard_dialog_has_symmetric_vertical_padding() {
    // The dialog must have: top border + leading spacer + prompt + spacer + buttons + trailing + bottom border = 7 total.
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};

    let s = SaveDiscardState::new("Save workspace changes before leaving?");
    let area = Rect::new(0, 0, 60, 7);
    let backend = TestBackend::new(area.width, area.height);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| render_save_discard_dialog(f, area, &s))
        .unwrap();
    let buf = term.backend().buffer();

    let row_text = |y: u16| {
        (0..area.width)
            .map(|x| buf[(x, y)].symbol().to_owned())
            .collect::<String>()
    };

    // Row 0: top border — must not contain prompt text.
    assert!(
        !row_text(0).contains("Save"),
        "prompt must not be on top border"
    );
    // Row 6: bottom border — must not contain button text.
    assert!(
        !row_text(6).contains("Save"),
        "buttons must not be on bottom border"
    );
    // Row 1: leading spacer — blank inside the border.
    let leading: String = row_text(1).chars().skip(1).take(58).collect();
    assert!(
        leading.trim().is_empty(),
        "row 1 must be blank leading spacer: {leading:?}"
    );
    // Row 5: trailing spacer — blank inside the border.
    let trailing: String = row_text(5).chars().skip(1).take(58).collect();
    assert!(
        trailing.trim().is_empty(),
        "row 5 must be blank trailing spacer: {trailing:?}"
    );
}
