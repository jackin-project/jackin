// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `account_picker`.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{AccountPickerKey, AccountPickerOutcome, AccountPickerState};

#[test]
fn account_picker_key_plan_moves_and_commits() {
    let mut picker = AccountPickerState::new("ctx", "agent", vec!["a", "b"]);

    assert_eq!(
        picker.handle_key(AccountPickerKey::Down),
        AccountPickerOutcome::Continue
    );
    assert_eq!(picker.selected(), 1);
    assert_eq!(
        picker.handle_key(AccountPickerKey::Down),
        AccountPickerOutcome::Continue
    );
    assert_eq!(picker.selected(), 1);
    assert_eq!(
        picker.handle_key(AccountPickerKey::Up),
        AccountPickerOutcome::Continue
    );
    assert_eq!(picker.selected(), 0);
    assert_eq!(
        picker.handle_key(AccountPickerKey::Commit),
        AccountPickerOutcome::Commit {
            context: "ctx",
            agent: "agent",
            provider: "a",
        }
    );
}

#[test]
fn account_picker_key_plan_cancels_and_ignores_other() {
    let mut picker = AccountPickerState::new(7, 11, vec![13]);

    assert_eq!(
        picker.handle_key(AccountPickerKey::Other),
        AccountPickerOutcome::Continue
    );
    assert_eq!(
        picker.handle_key(AccountPickerKey::Cancel),
        AccountPickerOutcome::Cancel
    );
}

#[test]
fn account_picker_key_maps_terminal_keys() {
    assert_eq!(
        AccountPickerKey::from(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)),
        AccountPickerKey::Up
    );
    assert_eq!(
        AccountPickerKey::from(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE,)),
        AccountPickerKey::Down
    );
    assert_eq!(
        AccountPickerKey::from(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        AccountPickerKey::Commit
    );
    assert_eq!(
        AccountPickerKey::from(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
        AccountPickerKey::Cancel
    );
    assert_eq!(
        AccountPickerKey::from(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE,)),
        AccountPickerKey::Other
    );
}
