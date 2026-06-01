use ratatui::layout::Rect;

use crate::console::tui::state::Modal;
use crate::console::tui::auth_panel;
use crate::selector::RolePickerState;
use jackin_console::tui::components::confirm_save;
use jackin_console::tui::components::modal_rects::{self, ModalRectSpec};

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
    if matches!(modal, Modal::MountDstChoice { .. }) {
        return mount_choice_rect(outer);
    }

    let (pct_w, height_rows) = match modal {
        Modal::TextInput { .. } => return text_input_rect(outer),
        Modal::Confirm { state, .. } => return confirm_rect(outer, state),
        Modal::SaveDiscardCancel { .. } => (70, 7),
        Modal::FileBrowser { .. } => (70, 22),
        Modal::WorkdirPick { .. } => (60, 12),
        Modal::MountDstChoice { .. } => unreachable!("handled above"),
        Modal::GithubPicker { state } => {
            let rows = (state.choices.len() as u16).saturating_add(5).min(15);
            (60, rows)
        }
        Modal::ConfirmSave { state } => {
            (80, confirm_save::required_height(state).min(outer.height))
        }
        Modal::ErrorPopup { state } => {
            let inner_width = (outer.width * 60 / 100).saturating_sub(4);
            let max_rows = outer.height.saturating_sub(2);
            (
                60,
                jackin_tui::components::error_dialog::required_height(state, inner_width, max_rows),
            )
        }
        Modal::ContainerInfo { state } => (
            60,
            jackin_tui::components::container_info_required_height(state),
        ),
        Modal::StatusPopup { .. } => (50, 7),
        Modal::OpPicker { state } if state.naming_stage_input().is_some() => {
            return text_input_rect(outer);
        }
        Modal::OpPicker { .. } => return op_picker_rect(outer),
        Modal::RolePicker { state }
        | Modal::RoleOverridePicker { state }
        | Modal::AuthRolePicker { state } => {
            return role_picker_rect(outer, state);
        }
        Modal::SourcePicker { .. } | Modal::AuthSourcePicker { .. } => {
            return source_picker_rect(outer);
        }
        Modal::ScopePicker { .. } => return scope_picker_rect(outer),
        Modal::AuthForm { state, .. } => return auth_form_rect(outer, state.as_ref()),
    };
    modal_rects::modal_rect(
        outer,
        ModalRectSpec::Fixed {
            width_pct: pct_w,
            height: height_rows,
        },
    )
}
