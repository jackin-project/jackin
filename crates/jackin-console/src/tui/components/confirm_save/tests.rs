// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `confirm_save`.
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

fn sample_state() -> ConfirmSaveState {
    ConfirmSaveState::new(vec![Line::from("Create workspace: demo")])
}

#[test]
fn confirm_save_defaults_to_cancel_focus() {
    // Default = Cancel so Enter on a freshly-opened dialog never fires
    // the save arm (TUI design decisions: confirmation dialog rule).
    let s = sample_state();
    assert_eq!(s.focus, ConfirmSaveFocus::Cancel);
}

#[test]
fn confirm_save_tab_cycles_cancel_save() {
    let mut s = sample_state();
    assert_eq!(s.focus, ConfirmSaveFocus::Cancel);
    assert!(matches!(
        s.handle_key(key(KeyCode::Tab)),
        ModalOutcome::Continue
    ));
    assert_eq!(s.focus, ConfirmSaveFocus::Save);
    s.handle_key(key(KeyCode::Tab));
    assert_eq!(s.focus, ConfirmSaveFocus::Cancel);
}

#[test]
fn confirm_save_left_cycles_reverse() {
    let mut s = sample_state();
    // Starts at Cancel; Left toggles to Save.
    s.handle_key(key(KeyCode::Left));
    assert_eq!(s.focus, ConfirmSaveFocus::Save);
    s.handle_key(key(KeyCode::Left));
    assert_eq!(s.focus, ConfirmSaveFocus::Cancel);
}

#[test]
fn confirm_save_enter_on_cancel_returns_cancel() {
    // Default focus = Cancel, so Enter fires Cancel immediately.
    let mut s = sample_state();
    assert!(matches!(
        s.handle_key(key(KeyCode::Enter)),
        ModalOutcome::Cancel
    ));
}

#[test]
fn confirm_save_enter_on_save_commits_save_choice() {
    // Tab once (Cancel -> Save) then Enter commits Save.
    let mut s = sample_state();
    s.handle_key(key(KeyCode::Tab)); // Cancel -> Save
    assert!(matches!(
        s.handle_key(key(KeyCode::Enter)),
        ModalOutcome::Commit(SaveChoice::Save)
    ));
}

#[test]
fn confirm_save_s_shortcut_commits_save() {
    let mut s = sample_state();
    // Rotate focus first to prove the shortcut is focus-independent.
    s.handle_key(key(KeyCode::Tab)); // Cancel -> Save
    assert!(matches!(
        s.handle_key(key(KeyCode::Char('s'))),
        ModalOutcome::Commit(SaveChoice::Save)
    ));

    let mut s = sample_state();
    assert!(matches!(
        s.handle_key(key(KeyCode::Char('S'))),
        ModalOutcome::Commit(SaveChoice::Save)
    ));
}

#[test]
fn confirm_save_c_shortcut_cancels() {
    let mut s = sample_state();
    assert!(matches!(
        s.handle_key(key(KeyCode::Char('c'))),
        ModalOutcome::Cancel
    ));

    let mut s = sample_state();
    assert!(matches!(
        s.handle_key(key(KeyCode::Char('C'))),
        ModalOutcome::Cancel
    ));
}

#[test]
fn confirm_save_esc_cancels() {
    let mut s = sample_state();
    assert!(matches!(
        s.handle_key(key(KeyCode::Esc)),
        ModalOutcome::Cancel
    ));
}

#[test]
fn required_height_accounts_for_chrome() {
    let s = ConfirmSaveState::<()>::new(vec![
        Line::from("one"),
        Line::from("two"),
        Line::from("three"),
    ]);
    // 3 content lines + 6 chrome rows (2 borders + leading + spacer + buttons + trailing)
    assert_eq!(required_height(&s), 9);
}

#[test]
fn confirm_save_scroll_keys_start_from_clamped_offset() {
    let mut s = ConfirmSaveState::<()>::new(vec![
        Line::from("one"),
        Line::from("two"),
        Line::from("three"),
        Line::from("four"),
    ]);
    s.preview_rows = 2;
    s.scroll_offset = 99;

    s.handle_key(key(KeyCode::Down));
    assert_eq!(s.scroll_offset, 2);

    s.handle_key(key(KeyCode::Up));
    assert_eq!(s.scroll_offset, 1);
}
