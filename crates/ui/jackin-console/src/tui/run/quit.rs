// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::QuitInterceptState;

#[must_use]
pub fn should_open_quit_confirm(
    key: crossterm::event::KeyEvent,
    state: QuitInterceptState,
) -> bool {
    use crossterm::event::{KeyCode, KeyModifiers};

    if !matches!(key.code, KeyCode::Char('q' | 'Q')) {
        return false;
    }
    let is_ctrl_q = key.modifiers.contains(KeyModifiers::CONTROL);
    let is_bare_q = (key.modifiers - KeyModifiers::SHIFT).is_empty()
        && !state.on_main_screen
        && !state.consumes_letter_input;
    is_ctrl_q || is_bare_q
}

#[must_use]
pub fn quit_confirm_state() -> crate::tui::components::ConfirmState {
    crate::tui::components::ConfirmState::new("Exit jackin❯?").with_focus_yes()
}

#[must_use]
pub fn should_open_keyboard_help(key: crossterm::event::KeyEvent) -> bool {
    use crossterm::event::{KeyCode, KeyModifiers};

    matches!(key.code, KeyCode::Char('?')) && (key.modifiers - KeyModifiers::SHIFT).is_empty()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuitConfirmPlan {
    Exit,
    Dismiss,
    Continue,
}

#[must_use]
pub const fn quit_confirm_plan(outcome: jackin_oppicker::ModalOutcome<bool>) -> QuitConfirmPlan {
    match outcome {
        jackin_oppicker::ModalOutcome::Commit(true) => QuitConfirmPlan::Exit,
        jackin_oppicker::ModalOutcome::Commit(false) | jackin_oppicker::ModalOutcome::Cancel => {
            QuitConfirmPlan::Dismiss
        }
        jackin_oppicker::ModalOutcome::Continue => QuitConfirmPlan::Continue,
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ModalBlockState {
    pub quit_confirm: bool,
    pub list_modal: bool,
    pub editor_modal: bool,
}

#[must_use]
pub const fn no_modal_blocks_base_surface(state: ModalBlockState) -> bool {
    !state.quit_confirm && !state.list_modal && !state.editor_modal
}

#[must_use]
pub const fn startup_error_was_dismissed(
    startup_error_pending: bool,
    list_modal_open: bool,
) -> bool {
    startup_error_pending && !list_modal_open
}

#[must_use]
pub const fn startup_error_modal_active(
    startup_error_pending: bool,
    list_modal_is_error_popup: bool,
) -> bool {
    startup_error_pending && list_modal_is_error_popup
}
