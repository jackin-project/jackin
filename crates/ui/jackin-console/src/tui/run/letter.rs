// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Letter-input modal plans.

use crate::tui::model::ConsoleManagerStageRoute;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuitInterceptState {
    pub on_main_screen: bool,
    pub consumes_letter_input: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LetterInputModalKind {
    TextInput,
    FilterPicker,
    Other,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LetterInputState {
    pub list_modal: Option<LetterInputModalKind>,
    pub editor_modal: Option<LetterInputModalKind>,
    pub create_prelude_modal: Option<LetterInputModalKind>,
    pub settings_mount_modal: Option<LetterInputModalKind>,
}

#[must_use]
pub const fn letter_input_state_for_route(
    route: ConsoleManagerStageRoute,
    list_modal: Option<LetterInputModalKind>,
    stage_modal: Option<LetterInputModalKind>,
) -> LetterInputState {
    let mut state = LetterInputState {
        list_modal,
        editor_modal: None,
        create_prelude_modal: None,
        settings_mount_modal: None,
    };
    match route {
        ConsoleManagerStageRoute::Editor => {
            state.editor_modal = stage_modal;
        }
        ConsoleManagerStageRoute::CreatePrelude => {
            state.create_prelude_modal = stage_modal;
        }
        ConsoleManagerStageRoute::Settings => {
            state.settings_mount_modal = stage_modal;
        }
        ConsoleManagerStageRoute::List
        | ConsoleManagerStageRoute::ConfirmDelete
        | ConsoleManagerStageRoute::ConfirmInstancePurge => {}
    }
    state
}

#[must_use]
pub const fn letter_input_modal_kind(
    text_input: bool,
    filter_picker: bool,
    modal_open: bool,
) -> Option<LetterInputModalKind> {
    if text_input {
        Some(LetterInputModalKind::TextInput)
    } else if filter_picker {
        Some(LetterInputModalKind::FilterPicker)
    } else if modal_open {
        Some(LetterInputModalKind::Other)
    } else {
        None
    }
}

/// Whether the active modal stack should receive bare letter keys.
///
/// The root console maps concrete modal variants into these generic facts.
/// Keeping the consumption policy here prevents the run loop from growing a
/// second copy of which component shapes type into filters or text inputs.
#[must_use]
pub const fn consumes_letter_input(state: LetterInputState) -> bool {
    modal_kind_consumes_letter_input(state.list_modal)
        || modal_kind_consumes_letter_input(state.editor_modal)
        || modal_kind_consumes_letter_input(state.create_prelude_modal)
        || modal_kind_consumes_letter_input(state.settings_mount_modal)
}

pub(crate) const fn modal_kind_consumes_letter_input(kind: Option<LetterInputModalKind>) -> bool {
    matches!(
        kind,
        Some(LetterInputModalKind::TextInput | LetterInputModalKind::FilterPicker)
    )
}
