// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `mount_dst_choice`.
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
fn new_defaults_focus_to_ok() {
    let s = MountDstChoiceState::new("/host/path");
    assert_eq!(s.focus, MountDstFocus::SamePath);
    assert_eq!(s.src, "/host/path");
}

#[test]
fn tab_cycles_ok_edit_cancel_ok() {
    let mut s = MountDstChoiceState::new("/h");
    assert_eq!(s.focus, MountDstFocus::SamePath);
    assert!(matches!(
        s.handle_key(key(KeyCode::Tab)),
        ModalOutcome::Continue
    ));
    assert_eq!(s.focus, MountDstFocus::Edit);
    s.handle_key(key(KeyCode::Tab));
    assert_eq!(s.focus, MountDstFocus::Cancel);
    s.handle_key(key(KeyCode::Tab));
    assert_eq!(s.focus, MountDstFocus::SamePath);
}

#[test]
fn left_reverse_cycles() {
    let mut s = MountDstChoiceState::new("/h");
    assert_eq!(s.focus, MountDstFocus::SamePath);
    s.handle_key(key(KeyCode::Left));
    assert_eq!(s.focus, MountDstFocus::Cancel);
    s.handle_key(key(KeyCode::Left));
    assert_eq!(s.focus, MountDstFocus::Edit);
    s.handle_key(key(KeyCode::Left));
    assert_eq!(s.focus, MountDstFocus::SamePath);
}

#[test]
fn enter_with_ok_focus_commits_ok() {
    let mut s = MountDstChoiceState::new("/h");
    assert!(matches!(
        s.handle_key(key(KeyCode::Enter)),
        ModalOutcome::Commit(MountDstChoice::SamePath)
    ));
}

#[test]
fn enter_with_edit_focus_commits_edit() {
    let mut s = MountDstChoiceState::new("/h");
    s.handle_key(key(KeyCode::Tab)); // Ok -> Edit
    assert!(matches!(
        s.handle_key(key(KeyCode::Enter)),
        ModalOutcome::Commit(MountDstChoice::Edit)
    ));
}

#[test]
fn enter_with_cancel_focus_returns_cancel() {
    let mut s = MountDstChoiceState::new("/h");
    s.handle_key(key(KeyCode::Tab)); // Ok -> Edit
    s.handle_key(key(KeyCode::Tab)); // Edit -> Cancel
    assert!(matches!(
        s.handle_key(key(KeyCode::Enter)),
        ModalOutcome::Cancel
    ));
}

#[test]
fn shortcut_m_commits_ok() {
    let mut s = MountDstChoiceState::new("/h");
    // Rotate focus away first to prove `m` is not focus-dependent.
    s.handle_key(key(KeyCode::Tab)); // focus -> Edit
    assert!(matches!(
        s.handle_key(key(KeyCode::Char('m'))),
        ModalOutcome::Commit(MountDstChoice::SamePath)
    ));
}

#[test]
fn shortcut_e_commits_edit() {
    let mut s = MountDstChoiceState::new("/h");
    assert!(matches!(
        s.handle_key(key(KeyCode::Char('e'))),
        ModalOutcome::Commit(MountDstChoice::Edit)
    ));
}

#[test]
fn shortcut_c_cancels() {
    let mut s = MountDstChoiceState::new("/h");
    assert!(matches!(
        s.handle_key(key(KeyCode::Char('c'))),
        ModalOutcome::Cancel
    ));
}

#[test]
fn esc_cancels() {
    let mut s = MountDstChoiceState::new("/h");
    assert!(matches!(
        s.handle_key(key(KeyCode::Esc)),
        ModalOutcome::Cancel
    ));
}

#[test]
fn uppercase_shortcuts_work() {
    // Shift-held shortcut characters should still route to the same
    // commit/cancel outcomes.
    let mut s = MountDstChoiceState::new("/h");
    assert!(matches!(
        s.handle_key(key(KeyCode::Char('M'))),
        ModalOutcome::Commit(MountDstChoice::SamePath)
    ));
    let mut s = MountDstChoiceState::new("/h");
    assert!(matches!(
        s.handle_key(key(KeyCode::Char('E'))),
        ModalOutcome::Commit(MountDstChoice::Edit)
    ));
    let mut s = MountDstChoiceState::new("/h");
    assert!(matches!(
        s.handle_key(key(KeyCode::Char('C'))),
        ModalOutcome::Cancel
    ));
}
