// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Console-state adapter plans.

use super::{
    LetterInputState, QuitInterceptState, console_screen_stage_for_route, consumes_letter_input,
    diagnostics_screen_for_stage, is_main_screen_for_route, letter_input_state_for_route,
    startup_error_modal_active, startup_error_was_dismissed,
};
use ratatui::layout::Rect;

#[must_use]
pub fn quit_confirm_area(frame: Rect, confirm: &crate::tui::components::ConfirmState) -> Rect {
    // Structural exception: the root console quit prompt is outside `Modal`; it still uses shared confirm height and centered geometry.
    let width: u16 = 44.min(frame.width.saturating_sub(4));
    let height: u16 = confirm
        .required_height()
        .min(frame.height.saturating_sub(2));
    let x = frame.x + frame.width.saturating_sub(width) / 2;
    let y = frame.y + frame.height.saturating_sub(height) / 2;
    Rect {
        x,
        y,
        width,
        height,
    }
}

// ── Concrete ConsoleState accessors ──────────────────────────────────────────
//
// These helpers extract facts from the concrete ConsoleState/ConsoleStage types
// that live in this crate. They avoid re-derivation in the root event loop.

pub const fn is_on_main_screen(state: &crate::tui::console::ConsoleState) -> bool {
    let crate::tui::console::ConsoleStage::Manager(ms) = &state.stage;
    is_main_screen_for_route(ms.stage.route(), ms.list_modal.is_some())
}

pub const fn screen_of(
    state: &crate::tui::console::ConsoleState,
) -> jackin_telemetry::schema::enums::ScreenId {
    let crate::tui::console::ConsoleStage::Manager(ms) = &state.stage;
    diagnostics_screen_for_stage(console_screen_stage_for_route(ms.stage.route()))
}

pub const fn letter_input_state_for_console(
    state: &crate::tui::console::ConsoleState,
) -> LetterInputState {
    use crate::tui::state::ManagerStage;
    let crate::tui::console::ConsoleStage::Manager(ms) = &state.stage;

    let list_modal = match &ms.list_modal {
        Some(modal) => modal.letter_input_kind(),
        None => None,
    };
    let stage_modal = match &ms.stage {
        ManagerStage::Editor(editor) => match &editor.modal {
            Some(modal) => modal.letter_input_kind(),
            None => None,
        },
        ManagerStage::CreatePrelude(prelude) => match &prelude.modal {
            Some(modal) => modal.letter_input_kind(),
            None => None,
        },
        ManagerStage::Settings(settings) => match settings.mounts.modals.current() {
            Some(modal) => modal.letter_input_kind(),
            None => None,
        },
        ManagerStage::List
        | ManagerStage::ConfirmDelete { .. }
        | ManagerStage::ConfirmInstancePurge { .. } => None,
    };

    letter_input_state_for_route(ms.stage.route(), list_modal, stage_modal)
}

pub const fn quit_intercept_state_for_console(
    state: &crate::tui::console::ConsoleState,
) -> QuitInterceptState {
    QuitInterceptState {
        on_main_screen: is_on_main_screen(state),
        consumes_letter_input: consumes_letter_input(letter_input_state_for_console(state)),
    }
}

pub fn no_modal_open(state: &crate::tui::console::ConsoleState) -> bool {
    state.base_surface_unblocked()
}

pub const fn startup_error_dismissed(
    state: &crate::tui::console::ConsoleState,
    startup_error_pending: bool,
) -> bool {
    let crate::tui::console::ConsoleStage::Manager(ms) = &state.stage;
    startup_error_was_dismissed(startup_error_pending, ms.list_modal.is_some())
}

pub fn startup_error_modal_active_for_console(
    state: &crate::tui::console::ConsoleState,
    startup_error_pending: bool,
) -> bool {
    let crate::tui::console::ConsoleStage::Manager(ms) = &state.stage;
    startup_error_modal_active(
        startup_error_pending,
        matches!(
            ms.list_modal,
            Some(crate::tui::state::Modal::ErrorPopup { .. })
        ),
    )
}
