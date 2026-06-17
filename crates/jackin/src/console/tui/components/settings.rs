//! Root-console settings display adapters.

use ratatui::Frame;

use crate::console::tui::state::{GlobalMountModal, SettingsAuthModal, SettingsEnvModal};
use jackin_console::tui::components::modal_rects;

pub(crate) fn render_global_mount_modal(frame: &mut Frame<'_>, modal: &GlobalMountModal<'_>) {
    let area = modal_rects::modal_rect_for_mode(frame.area(), modal.rect_mode());
    match modal {
        GlobalMountModal::Text { state, .. } => {
            jackin_tui::components::render_text_input(frame, area, state);
        }
        GlobalMountModal::FileBrowser { state } => {
            jackin_console::tui::components::file_browser::render(frame, area, state);
        }
        GlobalMountModal::MountDstChoice { state } => {
            jackin_console::tui::components::mount_dst_choice::render(frame, area, state);
        }
        GlobalMountModal::ScopePicker { state } => {
            jackin_console::tui::components::scope_picker::render(frame, area, state);
        }
        GlobalMountModal::RolePicker { state } => {
            jackin_console::tui::components::role_picker::render(frame, area, state);
        }
        GlobalMountModal::Confirm { state, .. } => {
            jackin_tui::components::render_confirm_dialog(frame, area, state);
        }
        GlobalMountModal::PreviewSave { state } => {
            jackin_console::tui::components::confirm_save::render(frame, area, state);
        }
    }
}

pub(crate) fn render_settings_env_modal(frame: &mut Frame<'_>, modal: &SettingsEnvModal<'_>) {
    let area = modal_rects::modal_rect_for_mode(frame.area(), modal.rect_mode());
    match modal {
        SettingsEnvModal::Text { state, .. } => {
            jackin_tui::components::render_text_input(frame, area, state);
        }
        SettingsEnvModal::SourcePicker { state } => {
            jackin_console::tui::components::source_picker::render(frame, area, state);
        }
        SettingsEnvModal::OpPicker { state } => {
            jackin_console::tui::components::op_picker::render_picker(frame, area, state.as_ref());
        }
        SettingsEnvModal::RolePicker { state } => {
            jackin_console::tui::components::role_picker::render(frame, area, state);
        }
        SettingsEnvModal::ScopePicker { state } => {
            jackin_console::tui::components::scope_picker::render(frame, area, state);
        }
        SettingsEnvModal::Confirm { state, .. } => {
            jackin_tui::components::render_confirm_dialog(frame, area, state);
        }
    }
}

pub(crate) fn render_settings_auth_modal(frame: &mut Frame<'_>, modal: &SettingsAuthModal<'_>) {
    let area = modal_rects::modal_rect_for_mode(frame.area(), modal.rect_mode());
    match modal {
        SettingsAuthModal::AuthForm { state, focus, .. } => {
            jackin_console::tui::components::auth_panel::render_form(frame, area, state, *focus);
        }
        SettingsAuthModal::SourcePicker { state } => {
            jackin_console::tui::components::source_picker::render(frame, area, state);
        }
        SettingsAuthModal::TextInput { state } => {
            jackin_tui::components::render_text_input(frame, area, state);
        }
        SettingsAuthModal::SourceFolderPicker { state } => {
            jackin_console::tui::components::file_browser::render(frame, area, state);
        }
        SettingsAuthModal::OpPicker { state } => {
            jackin_console::tui::components::op_picker::render_picker(frame, area, state.as_ref());
        }
    }
}
