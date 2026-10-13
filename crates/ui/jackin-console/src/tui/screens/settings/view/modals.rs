// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Settings modal render.

use ratatui::Frame;

use crate::tui::state::SettingsModal;

pub fn render_global_mount_modal(frame: &mut Frame<'_>, modal: &SettingsModal<'_>) {
    let area = modal.rect(frame.area());
    match modal {
        SettingsModal::MountText { state, .. } => {
            crate::tui::components::render_text_input(frame, area, state);
        }
        SettingsModal::MountFileBrowser { state } => {
            crate::tui::components::file_browser::render(frame, area, state);
        }
        SettingsModal::MountDstChoice { state } => {
            crate::tui::components::mount_dst_choice::render(frame, area, state);
        }
        SettingsModal::MountScopePicker { state } => {
            crate::tui::components::scope_picker::render(frame, area, state);
        }
        SettingsModal::MountRolePicker { state } => {
            crate::tui::components::role_picker::render(frame, area, state);
        }
        SettingsModal::MountConfirm { state, .. } => {
            crate::tui::components::render_confirm_dialog(frame, area, state);
        }
        SettingsModal::MountPreviewSave { state } => {
            crate::tui::components::confirm_save::render(frame, area, state);
        }
        _ => unreachable!("mount renderer received a non-mount settings modal"),
    }
}

pub fn render_settings_env_modal(frame: &mut Frame<'_>, modal: &SettingsModal<'_>) {
    let area = modal.rect(frame.area());
    match modal {
        SettingsModal::EnvText { state, .. } => {
            crate::tui::components::render_text_input(frame, area, state);
        }
        SettingsModal::EnvSourcePicker { state, .. } => {
            crate::tui::components::source_picker::render(frame, area, state);
        }
        SettingsModal::EnvOpPicker { state, .. } => {
            crate::tui::components::op_picker::render_picker(frame, area, state.as_ref());
        }
        SettingsModal::EnvRolePicker { state } => {
            crate::tui::components::role_picker::render(frame, area, state);
        }
        SettingsModal::EnvScopePicker { state } => {
            crate::tui::components::scope_picker::render(frame, area, state);
        }
        SettingsModal::EnvConfirm { state, .. } => {
            crate::tui::components::render_confirm_dialog(frame, area, state);
        }
        _ => unreachable!("env renderer received a non-env settings modal"),
    }
}

pub fn render_settings_auth_modal(frame: &mut Frame<'_>, modal: &SettingsModal<'_>) {
    let area = modal.rect(frame.area());
    match modal {
        SettingsModal::AuthForm { state, focus, .. } => {
            crate::tui::components::auth_panel::render_form(frame, area, state, *focus);
        }
        SettingsModal::AuthSourcePicker { state } => {
            crate::tui::components::source_picker::render(frame, area, state);
        }
        SettingsModal::AuthTextInput { state } => {
            crate::tui::components::render_text_input(frame, area, state);
        }
        SettingsModal::AuthSourceFolderPicker { state } => {
            crate::tui::components::file_browser::render(frame, area, state);
        }
        SettingsModal::AuthOpPicker { state } => {
            crate::tui::components::op_picker::render_picker(frame, area, state.as_ref());
        }
        _ => unreachable!("auth renderer received a non-auth settings modal"),
    }
}
