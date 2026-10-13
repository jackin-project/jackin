// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `dialogs` — the behavior contracts plan 010 step 1 pins before
//! any upstream-widget cutover. Expected values come from pre-cutover
//! behavior, asserted literally.

use super::*;
use crossterm::event::{KeyCode as CrosstermKeyCode, KeyEventKind, KeyEventState, KeyModifiers};
use jackin_oppicker::ModalOutcome;

fn key(code: CrosstermKeyCode) -> KeyEvent {
    crossterm::event::KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
    .into()
}

// Spec scenario "Confirm default focus preserved": destructive confirms rest
// on No, so Enter at rest never confirms.
#[test]
fn workspace_delete_confirm_rests_no_focused() {
    let mut state = crate::tui::screens::workspaces::update::workspace_delete_confirm_state("demo");
    let outcome = state.handle_key(key(CrosstermKeyCode::Enter));
    assert_eq!(outcome, ModalOutcome::Commit(false));
}

#[test]
fn instance_purge_confirm_rests_no_focused() {
    let mut state = crate::tui::screens::workspaces::update::instance_purge_confirm_state("demo");
    let outcome = state.handle_key(key(CrosstermKeyCode::Enter));
    assert_eq!(outcome, ModalOutcome::Commit(false));
}

// The non-destructive exit confirm is the intentional Yes-focused exception.
#[test]
fn quit_confirm_rests_yes_focused() {
    let mut state = crate::tui::run::quit_confirm_state();
    let outcome = state.handle_key(key(CrosstermKeyCode::Enter));
    assert_eq!(outcome, ModalOutcome::Commit(true));
}

// Esc and direct keys: Esc cancels, y commits confirm, n commits cancel,
// Tab/BackTab move focus without committing.
#[test]
fn confirm_esc_cancels() {
    let mut state = ConfirmState::new("Delete \"demo\"?");
    let outcome = state.handle_key(key(CrosstermKeyCode::Esc));
    assert_eq!(outcome, ModalOutcome::Cancel);
}

#[test]
fn confirm_direct_y_commits_confirm() {
    let mut state = ConfirmState::new("Delete \"demo\"?");
    let outcome = state.handle_key(key(CrosstermKeyCode::Char('y')));
    assert_eq!(outcome, ModalOutcome::Commit(true));
}

#[test]
fn confirm_direct_n_commits_cancel_choice() {
    let mut state = ConfirmState::new("Delete \"demo\"?");
    let outcome = state.handle_key(key(CrosstermKeyCode::Char('n')));
    assert_eq!(outcome, ModalOutcome::Commit(false));
}

#[test]
fn confirm_tab_moves_focus_without_committing() {
    let mut state = ConfirmState::new("Delete \"demo\"?");
    let outcome = state.handle_key(key(CrosstermKeyCode::Tab));
    assert_eq!(outcome, ModalOutcome::Continue);
    // Focus moved off the No rest position: Enter now commits Yes.
    let outcome = state.handle_key(key(CrosstermKeyCode::Enter));
    assert_eq!(outcome, ModalOutcome::Commit(true));
}

#[test]
fn confirm_backtab_from_rest_wraps_to_yes() {
    let mut state = ConfirmState::new("Delete \"demo\"?");
    let outcome = state.handle_key(key(CrosstermKeyCode::BackTab));
    assert_eq!(outcome, ModalOutcome::Continue);
    let outcome = state.handle_key(key(CrosstermKeyCode::Enter));
    assert_eq!(outcome, ModalOutcome::Commit(true));
}

// Save/discard/cancel: s/d commit their choices, Esc/c cancels, resting
// focus is Cancel.
#[test]
fn save_discard_rests_cancel_focused() {
    let mut state = SaveDiscardState::new("Save changes?");
    let outcome = state.handle_key(key(CrosstermKeyCode::Enter));
    assert_eq!(outcome, ModalOutcome::Cancel);
}

#[test]
fn save_discard_direct_s_commits_save() {
    let mut state = SaveDiscardState::new("Save changes?");
    let outcome = state.handle_key(key(CrosstermKeyCode::Char('s')));
    assert_eq!(outcome, ModalOutcome::Commit(SaveDiscardChoice::Save));
}

#[test]
fn save_discard_direct_d_commits_discard() {
    let mut state = SaveDiscardState::new("Save changes?");
    let outcome = state.handle_key(key(CrosstermKeyCode::Char('d')));
    assert_eq!(outcome, ModalOutcome::Commit(SaveDiscardChoice::Discard));
}

#[test]
fn save_discard_esc_and_c_cancel() {
    let mut state = SaveDiscardState::new("Save changes?");
    let outcome = state.handle_key(key(CrosstermKeyCode::Esc));
    assert_eq!(outcome, ModalOutcome::Cancel);
    let outcome = state.handle_key(key(CrosstermKeyCode::Char('c')));
    assert_eq!(outcome, ModalOutcome::Cancel);
}

// Error popup: Enter/Esc/o dismiss; anything else is inert.
#[test]
fn error_popup_dismiss_keys() {
    let mut state = ErrorPopupState::new("Error", "boom");
    for code in [
        CrosstermKeyCode::Enter,
        CrosstermKeyCode::Esc,
        CrosstermKeyCode::Char('o'),
    ] {
        let outcome = state.handle_key(key(code));
        assert_eq!(outcome, ModalOutcome::Cancel);
    }
    let outcome = state.handle_key(key(CrosstermKeyCode::Char('x')));
    assert_eq!(outcome, ModalOutcome::Continue);
}
