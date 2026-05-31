//! Modal dispatcher: widget-dispatch wrapper (`render_modal`) that draws the
//! active modal at the manager-owned computed geometry.

use ratatui::Frame;

use super::super::super::widgets::{
    auth_panel, confirm_save, file_browser, github_picker, mount_dst_choice, op_picker,
    role_picker, scope_picker, source_picker, workdir_pick,
};
use crate::console::manager::modal_layout::modal_outer_rect;
use crate::console::manager::state::Modal;

// ── Modal dispatcher ────────────────────────────────────────────────

pub(super) fn render_modal(frame: &mut Frame, modal: &Modal<'_>) {
    let area = frame.area();
    let modal_area = modal_outer_rect(modal, area);
    match modal {
        Modal::TextInput { state, .. } => {
            jackin_tui::components::render_text_input(frame, modal_area, state);
        }
        Modal::FileBrowser { state, .. } => file_browser::render(frame, modal_area, state),
        Modal::WorkdirPick { state } => workdir_pick::render(frame, modal_area, state),
        Modal::Confirm { state, .. } => {
            jackin_tui::components::render_confirm_dialog(frame, modal_area, state);
        }
        Modal::SaveDiscardCancel { state } => {
            jackin_tui::components::render_save_discard_dialog(frame, modal_area, state);
        }
        Modal::MountDstChoice { state, .. } => {
            mount_dst_choice::render(frame, modal_area, state);
        }
        Modal::GithubPicker { state } => github_picker::render(frame, modal_area, state),
        Modal::ConfirmSave { state } => confirm_save::render(frame, modal_area, state),
        Modal::ErrorPopup { state } => {
            jackin_tui::components::render_error_dialog(frame, modal_area, state);
        }
        Modal::ContainerInfo { state } => {
            jackin_tui::components::render_container_info(frame, modal_area, state);
        }
        Modal::StatusPopup { state } => {
            jackin_tui::components::render_status_popup(frame, modal_area, state);
        }
        Modal::OpPicker { state } => {
            op_picker::render::render(frame, modal_area, state);
        }
        Modal::RolePicker { state }
        | Modal::RoleOverridePicker { state }
        | Modal::AuthRolePicker { state } => {
            role_picker::render(frame, modal_area, state);
        }
        Modal::SourcePicker { state, .. } | Modal::AuthSourcePicker { state } => {
            source_picker::render(frame, modal_area, state);
        }
        Modal::ScopePicker { state } => scope_picker::render(frame, modal_area, state),
        Modal::AuthForm { state, focus, .. } => {
            auth_panel::render_form(frame, modal_area, state.as_ref(), *focus);
        }
    }
}
