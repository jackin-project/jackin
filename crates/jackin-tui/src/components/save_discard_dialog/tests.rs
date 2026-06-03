//! Tests for `save_discard_dialog`.
use super::*;
use crossterm::event::{KeyEventKind, KeyEventState, KeyModifiers};

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
    let _ = s.handle_key(key(KeyCode::Tab));
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
