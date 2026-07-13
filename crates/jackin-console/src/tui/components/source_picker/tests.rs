// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `source_picker`.
use super::*;
use crossterm::event::{KeyEventKind, KeyEventState, KeyModifiers};

const fn key_event(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

#[test]
fn source_picker_default_focus_is_plain() {
    let s = SourcePickerState::new("MY_KEY".into(), true);
    assert_eq!(s.focused, SourceChoice::Plain);
}

#[test]
fn source_picker_right_arrow_advances_to_op_when_available() {
    let mut s = SourcePickerState::new("MY_KEY".into(), true);
    drop(s.handle_key(key_event(KeyCode::Right)));
    assert_eq!(s.focused, SourceChoice::Op);
}

#[test]
fn source_picker_right_arrow_skips_op_when_unavailable() {
    let mut s = SourcePickerState::new("MY_KEY".into(), false);
    drop(s.handle_key(key_event(KeyCode::Right)));
    assert_eq!(
        s.focused,
        SourceChoice::Plain,
        "cycling must skip the disabled Op button when op is unavailable"
    );
    drop(s.handle_key(key_event(KeyCode::Right)));
    drop(s.handle_key(key_event(KeyCode::Tab)));
    drop(s.handle_key(key_event(KeyCode::Char('l'))));
    assert_eq!(s.focused, SourceChoice::Plain);
}

#[test]
fn source_picker_enter_on_plain_commits_plain() {
    let mut s = SourcePickerState::new("MY_KEY".into(), true);
    assert!(matches!(
        s.handle_key(key_event(KeyCode::Enter)),
        ModalOutcome::Commit(SourceChoice::Plain)
    ));
}

#[test]
fn source_picker_enter_on_op_when_available_commits_op() {
    let mut s = SourcePickerState::new("MY_KEY".into(), true);
    drop(s.handle_key(key_event(KeyCode::Right)));
    assert_eq!(s.focused, SourceChoice::Op);
    assert!(matches!(
        s.handle_key(key_event(KeyCode::Enter)),
        ModalOutcome::Commit(SourceChoice::Op)
    ));
}

#[test]
fn source_picker_esc_returns_cancel() {
    let mut s = SourcePickerState::new("MY_KEY".into(), true);
    assert!(matches!(
        s.handle_key(key_event(KeyCode::Esc)),
        ModalOutcome::Cancel
    ));
}

#[test]
fn source_picker_o_hotkey_inert_when_op_unavailable() {
    let mut s = SourcePickerState::new("MY_KEY".into(), false);
    assert!(matches!(
        s.handle_key(key_event(KeyCode::Char('O'))),
        ModalOutcome::Continue
    ));
    assert_eq!(s.focused, SourceChoice::Plain);
}
