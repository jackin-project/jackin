// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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

#[test]
fn quit_confirm_rests_yes_focused() {
    let mut state = crate::tui::run::quit_confirm_state();
    let outcome = state.handle_key(key(CrosstermKeyCode::Enter));
    assert_eq!(outcome, ModalOutcome::Commit(true));
}

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
