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

#[test]
fn confirm_height_contains_every_body_row_and_action() {
    let state = ConfirmState::details(
        "Trust role", "Confirm?",
        vec![("Role".into(), "alpha".into()), ("Repository".into(), "repo".into())],
        vec!["First safety note".into(), "Final safety note".into()],
    );
    assert_eq!(state.required_height(), 13);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 20))
        .expect("test terminal");
    terminal.draw(|frame| {
        render_confirm_dialog(frame, Rect::new(0, 0, 70, state.required_height()), &state);
    }).expect("draw confirm");
    let buffer = terminal.backend().buffer();
    let visible = (0..buffer.area.height).map(|y| {
        (0..buffer.area.width).map(|x| buffer[(x, y)].symbol()).collect::<String>()
    }).collect::<Vec<_>>().join("\n");
    assert!(visible.contains("First safety note"), "{visible}");
    assert!(visible.contains("Final safety note"), "{visible}");
    assert!(visible.contains("Yes") && visible.contains("No"), "{visible}");
}

#[test]
fn confirm_height_measures_the_painted_multiline_details() {
    let state = ConfirmState::details(
        "Trust role", "Confirm?\nReview first",
        vec![("Role".into(), "alpha\nbeta".into())],
        vec!["First note\nFinal note".into()],
    );
    assert_eq!(confirm_text(&state).lines.len(), 8);
    assert_eq!(state.required_height(), 14);
}

#[test]
fn error_height_keeps_both_final_message_rows_visible() {
    let state = ErrorPopupState::new("Failure", "First line\nMiddle line\nFinal line");
    assert_eq!(state.required_height(60, 20), 9);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 20))
        .expect("test terminal");
    terminal.draw(|frame| {
        render_error_dialog(frame, Rect::new(0, 0, 70, state.required_height(60, 20)), &state);
    }).expect("draw error");
    let buffer = terminal.backend().buffer();
    let visible = (0..buffer.area.height).map(|y| {
        (0..buffer.area.width).map(|x| buffer[(x, y)].symbol()).collect::<String>()
    }).collect::<Vec<_>>().join("\n");
    assert!(visible.contains("Middle line"), "{visible}");
    assert!(visible.contains("Final line"), "{visible}");
}
