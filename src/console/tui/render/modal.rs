//! Modal dispatcher: widget-dispatch wrapper (`render_modal`) that draws the
//! active modal at the manager-owned computed geometry.

use ratatui::Frame;

use crate::console::tui::render::modal_layout::modal_outer_rect;
use crate::console::tui::state::Modal;
use crate::console::tui::auth_panel;

// ── Modal dispatcher ────────────────────────────────────────────────

pub(super) fn render_modal(frame: &mut Frame, modal: &Modal<'_>) {
    let area = frame.area();
    let modal_area = modal_outer_rect(modal, area);
    match modal {
        Modal::TextInput { state, .. } => {
            jackin_tui::components::render_text_input(frame, modal_area, state);
        }
        Modal::FileBrowser { state, .. } => {
            jackin_console::tui::components::file_browser::render(frame, modal_area, state);
        }
        Modal::WorkdirPick { state } => {
            jackin_console::tui::components::workdir_pick::render(frame, modal_area, state);
        }
        Modal::Confirm { state, .. } => {
            jackin_tui::components::render_confirm_dialog(frame, modal_area, state);
        }
        Modal::SaveDiscardCancel { state } => {
            jackin_tui::components::render_save_discard_dialog(frame, modal_area, state);
        }
        Modal::MountDstChoice { state, .. } => {
            jackin_console::tui::components::mount_dst_choice::render(frame, modal_area, state);
        }
        Modal::GithubPicker { state } => {
            jackin_console::tui::components::github_picker::render(frame, modal_area, state);
        }
        Modal::ConfirmSave { state } => {
            jackin_console::tui::components::confirm_save::render(frame, modal_area, state);
        }
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
            jackin_console::tui::components::op_picker::render_picker(
                frame,
                modal_area,
                state.as_ref(),
            );
        }
        Modal::RolePicker { state }
        | Modal::RoleOverridePicker { state }
        | Modal::AuthRolePicker { state } => {
            jackin_console::tui::components::role_picker::render(frame, modal_area, state);
        }
        Modal::SourcePicker { state, .. } | Modal::AuthSourcePicker { state } => {
            jackin_console::tui::components::source_picker::render(frame, modal_area, state);
        }
        Modal::ScopePicker { state } => {
            jackin_console::tui::components::scope_picker::render(frame, modal_area, state);
        }
        Modal::AuthForm { state, focus, .. } => {
            auth_panel::render_form(frame, modal_area, state.as_ref(), *focus);
        }
    }
}
