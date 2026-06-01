use ratatui::layout::Rect;

use crate::console::tui::state::Modal;
use crate::console::tui::auth_panel;
use crate::selector::RolePickerState;
use jackin_console::tui::components::confirm_save;
use jackin_console::tui::components::modal_rects::{self, ModalRectMode, ModalRectSpec};

pub(crate) fn text_input_rect(outer: Rect) -> Rect {
    modal_rects::modal_rect(outer, ModalRectSpec::TextInput)
}

pub(crate) fn source_picker_rect(outer: Rect) -> Rect {
    modal_rects::modal_rect(outer, ModalRectSpec::SourcePicker)
}

pub(crate) fn scope_picker_rect(outer: Rect) -> Rect {
    modal_rects::modal_rect(outer, ModalRectSpec::ScopePicker)
}

pub(crate) fn op_picker_rect(outer: Rect) -> Rect {
    modal_rects::modal_rect(outer, ModalRectSpec::OpPicker)
}

pub(crate) fn role_picker_rect(outer: Rect, state: &RolePickerState) -> Rect {
    modal_rects::modal_rect(
        outer,
        ModalRectSpec::RolePicker {
            filtered_len: state.filtered.len(),
        },
    )
}

pub(crate) fn confirm_rect(outer: Rect, state: &jackin_tui::components::ConfirmState) -> Rect {
    modal_rects::modal_rect(
        outer,
        ModalRectSpec::Confirm {
            width_pct: jackin_tui::components::confirm_width_pct(state),
            height: jackin_tui::components::confirm_required_height(state),
        },
    )
}

pub(crate) fn mount_choice_rect(outer: Rect) -> Rect {
    modal_rects::modal_rect(outer, ModalRectSpec::MountChoice)
}

pub(crate) fn auth_form_rect(outer: Rect, state: &auth_panel::AuthForm) -> Rect {
    modal_rects::modal_rect(
        outer,
        ModalRectSpec::AuthForm {
            required_height: auth_panel::required_height(state),
        },
    )
}

/// Single source of truth for modal size and placement.
pub(crate) fn modal_outer_rect(modal: &Modal<'_>, outer: Rect) -> Rect {
    let mode = match modal {
        Modal::TextInput { .. } => ModalRectMode::TextInput,
        Modal::Confirm { state, .. } => ModalRectMode::Confirm {
            width_pct: jackin_tui::components::confirm_width_pct(state),
            height: jackin_tui::components::confirm_required_height(state),
        },
        Modal::SaveDiscardCancel { .. } => ModalRectMode::SaveDiscardCancel,
        Modal::FileBrowser { .. } => ModalRectMode::FileBrowser,
        Modal::WorkdirPick { .. } => ModalRectMode::WorkdirPick,
        Modal::MountDstChoice { .. } => ModalRectMode::MountChoice,
        Modal::GithubPicker { state } => ModalRectMode::GithubPicker {
            choice_len: state.choices.len(),
        },
        Modal::ConfirmSave { state } => ModalRectMode::ConfirmSave {
            required_height: confirm_save::required_height(state),
        },
        Modal::ErrorPopup { state } => {
            let inner_width = (outer.width * 60 / 100).saturating_sub(4);
            let max_rows = outer.height.saturating_sub(2);
            ModalRectMode::ErrorPopup {
                required_height: jackin_tui::components::error_dialog::required_height(
                    state,
                    inner_width,
                    max_rows,
                ),
            }
        }
        Modal::ContainerInfo { state } => ModalRectMode::ContainerInfo {
            required_height: jackin_tui::components::container_info_required_height(state),
        },
        Modal::StatusPopup { .. } => ModalRectMode::StatusPopup,
        Modal::OpPicker { state } if state.naming_stage_input().is_some() => {
            ModalRectMode::TextInput
        }
        Modal::OpPicker { .. } => ModalRectMode::OpPicker,
        Modal::RolePicker { state }
        | Modal::RoleOverridePicker { state }
        | Modal::AuthRolePicker { state } => ModalRectMode::RolePicker {
            filtered_len: state.filtered.len(),
        },
        Modal::SourcePicker { .. } | Modal::AuthSourcePicker { .. } => {
            ModalRectMode::SourcePicker
        }
        Modal::ScopePicker { .. } => ModalRectMode::ScopePicker,
        Modal::AuthForm { state, .. } => ModalRectMode::AuthForm {
            required_height: auth_panel::required_height(state.as_ref()),
        }
    };
    modal_rects::modal_rect_for_mode(outer, mode)
}
