//! Modal geometry shared by render and input hit-testing.

use jackin_console::layout::centered_rect_fixed;
use ratatui::layout::Rect;

use crate::console::manager::state::Modal;
use crate::console::widgets::{auth_panel, confirm_save, role_picker};

pub(crate) fn text_input_rect(outer: Rect) -> Rect {
    centered_rect_fixed(outer, 60, 5)
}

pub(crate) fn source_picker_rect(outer: Rect) -> Rect {
    centered_rect_fixed(outer, 50, 5)
}

pub(crate) fn scope_picker_rect(outer: Rect) -> Rect {
    centered_rect_fixed(outer, 50, 5)
}

pub(crate) fn op_picker_rect(outer: Rect) -> Rect {
    centered_rect_fixed(outer, 80, 22)
}

pub(crate) fn role_picker_rect(outer: Rect, state: &role_picker::RolePickerState) -> Rect {
    let rows = (state.filtered.len() as u16).saturating_add(6).min(15);
    centered_rect_fixed(outer, 50, rows)
}

pub(crate) fn confirm_rect(outer: Rect, state: &jackin_tui::components::ConfirmState) -> Rect {
    centered_rect_fixed(
        outer,
        jackin_tui::components::confirm_width_pct(state),
        jackin_tui::components::confirm_required_height(state),
    )
}

pub(crate) fn mount_choice_rect(outer: Rect) -> Rect {
    let w = outer.width.min(80);
    let h = 6.min(outer.height);
    Rect {
        x: outer.x + outer.width.saturating_sub(w) / 2,
        y: outer.y + outer.height.saturating_sub(h) / 2,
        width: w,
        height: h,
    }
}

pub(crate) fn auth_form_rect(outer: Rect, state: &auth_panel::AuthForm) -> Rect {
    centered_rect_fixed(outer, 80, auth_panel::required_height(state))
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
    centered_rect_fixed(outer, pct_w, height_rows)
}
