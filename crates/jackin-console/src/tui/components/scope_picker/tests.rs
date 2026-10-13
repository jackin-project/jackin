// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `scope_picker`.
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
fn scope_picker_default_focus_is_all_agents() {
    let s = ScopePickerState::new();
    assert_eq!(s.focused, ScopeChoice::AllAgents);
}

#[test]
fn scope_picker_right_arrow_advances_to_specific() {
    let mut s = ScopePickerState::new();
    drop(s.handle_key(key_event(KeyCode::Right)));
    assert_eq!(s.focused, ScopeChoice::SpecificAgent);
}

#[test]
fn scope_picker_enter_on_all_commits_all() {
    let mut s = ScopePickerState::new();
    assert!(matches!(
        s.handle_key(key_event(KeyCode::Enter)),
        ModalOutcome::Commit(ScopeChoice::AllAgents)
    ));
}

#[test]
fn scope_picker_enter_on_specific_commits_specific() {
    let mut s = ScopePickerState::new();
    drop(s.handle_key(key_event(KeyCode::Right)));
    assert_eq!(s.focused, ScopeChoice::SpecificAgent);
    assert!(matches!(
        s.handle_key(key_event(KeyCode::Enter)),
        ModalOutcome::Commit(ScopeChoice::SpecificAgent)
    ));
}

#[test]
fn scope_picker_esc_cancels() {
    let mut s = ScopePickerState::new();
    assert!(matches!(
        s.handle_key(key_event(KeyCode::Esc)),
        ModalOutcome::Cancel
    ));
}

#[test]
fn scope_picker_left_arrow_toggles_back_to_all_agents() {
    let mut s = ScopePickerState::new();
    drop(s.handle_key(key_event(KeyCode::Right)));
    assert_eq!(s.focused, ScopeChoice::SpecificAgent);
    drop(s.handle_key(key_event(KeyCode::Left)));
    assert_eq!(s.focused, ScopeChoice::AllAgents);
}
