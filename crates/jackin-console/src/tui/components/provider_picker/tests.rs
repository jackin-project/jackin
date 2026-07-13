// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `provider_picker`.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{ProviderPickerKey, ProviderPickerOutcome, ProviderPickerState};

#[test]
fn provider_picker_key_plan_moves_and_commits() {
    let mut picker = ProviderPickerState::new("ctx", "agent", vec!["a", "b"]);

    assert_eq!(
        picker.handle_key(ProviderPickerKey::Down),
        ProviderPickerOutcome::Continue
    );
    assert_eq!(picker.selected(), 1);
    assert_eq!(
        picker.handle_key(ProviderPickerKey::Down),
        ProviderPickerOutcome::Continue
    );
    assert_eq!(picker.selected(), 1);
    assert_eq!(
        picker.handle_key(ProviderPickerKey::Up),
        ProviderPickerOutcome::Continue
    );
    assert_eq!(picker.selected(), 0);
    assert_eq!(
        picker.handle_key(ProviderPickerKey::Commit),
        ProviderPickerOutcome::Commit {
            context: "ctx",
            agent: "agent",
            provider: "a",
        }
    );
}

#[test]
fn provider_picker_key_plan_cancels_and_ignores_other() {
    let mut picker = ProviderPickerState::new(7, 11, vec![13]);

    assert_eq!(
        picker.handle_key(ProviderPickerKey::Other),
        ProviderPickerOutcome::Continue
    );
    assert_eq!(
        picker.handle_key(ProviderPickerKey::Cancel),
        ProviderPickerOutcome::Cancel
    );
}

#[test]
fn provider_picker_key_maps_terminal_keys() {
    assert_eq!(
        ProviderPickerKey::from(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)),
        ProviderPickerKey::Up
    );
    assert_eq!(
        ProviderPickerKey::from(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE,)),
        ProviderPickerKey::Down
    );
    assert_eq!(
        ProviderPickerKey::from(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        ProviderPickerKey::Commit
    );
    assert_eq!(
        ProviderPickerKey::from(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
        ProviderPickerKey::Cancel
    );
    assert_eq!(
        ProviderPickerKey::from(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE,)),
        ProviderPickerKey::Other
    );
}
