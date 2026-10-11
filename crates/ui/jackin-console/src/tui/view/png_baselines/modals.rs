// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Modal baseline builders.
#![cfg(test)]

use super::*;
use std::path::PathBuf;

use crate::tui::state::{EditorState, ManagerStage, ManagerState, Modal};
use jackin_config::{AppConfig, WorkspaceConfig};

pub(crate) fn with_list_modal(
    modal: Modal<'static>,
) -> (ManagerState<'static>, AppConfig, PathBuf) {
    let config = populated_config();
    let cwd = test_cwd();
    let mut state = ManagerState::from_config(&config, &cwd);
    state.list_modal = Some(modal);
    (state, config, cwd)
}

pub(crate) fn with_editor_modal(
    modal: Modal<'static>,
) -> (ManagerState<'static>, AppConfig, PathBuf) {
    let config = populated_config();
    let cwd = test_cwd();
    let mut state = ManagerState::from_config(&config, &cwd);
    let mut editor = EditorState::new_edit("alpha".into(), WorkspaceConfig::default());
    editor.modal = Some(modal);
    state.stage = ManagerStage::Editor(editor);
    (state, config, cwd)
}

pub(crate) fn role_picker_state() -> crate::tui::state::RolePickerState {
    crate::tui::state::RolePickerState::new(vec![
        jackin_core::RoleSelector::parse("chainargos/agent-smith").expect("valid role selector"),
    ])
}

pub(crate) fn modal_text_input() -> (ManagerState<'static>, AppConfig, PathBuf) {
    with_editor_modal(Modal::TextInput {
        target: crate::tui::state::TextInputTarget::Name,
        state: crate::tui::components::TextInputState::new("Name", "alpha"),
    })
}

pub(crate) fn modal_file_browser() -> (ManagerState<'static>, AppConfig, PathBuf) {
    let cwd = test_cwd();
    with_list_modal(Modal::FileBrowser {
        target: crate::tui::state::FileBrowserTarget::CreateFirstMountSrc,
        state: crate::tui::components::file_browser::FileBrowserState::from_listing(
            crate::services::file_browser::listing_at(cwd.clone(), cwd),
        ),
    })
}

pub(crate) fn modal_mount_dst_choice() -> (ManagerState<'static>, AppConfig, PathBuf) {
    with_list_modal(Modal::MountDstChoice {
        target: crate::tui::state::FileBrowserTarget::CreateFirstMountSrc,
        state: crate::tui::components::mount_dst_choice::MountDstChoiceState::new("/workspace"),
    })
}

pub(crate) fn modal_workdir_pick() -> (ManagerState<'static>, AppConfig, PathBuf) {
    with_list_modal(Modal::WorkdirPick {
        state: crate::tui::components::workdir_pick::WorkdirPickState::from_mounts(&[
            jackin_config::MountConfig {
                src: "/workspace".into(),
                dst: "/workspace".into(),
                readonly: false,
                isolation: jackin_config::MountIsolation::Shared,
            },
        ]),
    })
}

pub(crate) fn modal_confirm() -> (ManagerState<'static>, AppConfig, PathBuf) {
    with_list_modal(Modal::Confirm {
        target: crate::tui::state::ConfirmTarget::DeleteEnvVar {
            scope: crate::tui::state::SecretsScopeTag::Workspace,
            key: "TOKEN".into(),
        },
        state: crate::tui::components::ConfirmState::new("Delete TOKEN?"),
    })
}

pub(crate) fn modal_save_discard_cancel() -> (ManagerState<'static>, AppConfig, PathBuf) {
    with_list_modal(Modal::SaveDiscardCancel {
        state: crate::tui::components::SaveDiscardState::new("Save changes?"),
    })
}

pub(crate) fn modal_github_picker() -> (ManagerState<'static>, AppConfig, PathBuf) {
    with_list_modal(Modal::GithubPicker {
        state: crate::tui::components::github_picker::GithubPickerState::new(vec![
            crate::github_mounts::GithubChoice {
                src: "/workspace".into(),
                branch: "main".into(),
                url: "https://github.com/example/repo".into(),
            },
        ]),
    })
}

pub(crate) fn modal_confirm_save() -> (ManagerState<'static>, AppConfig, PathBuf) {
    use ratatui::text::Line;
    with_list_modal(
        Modal::ConfirmSave {
            state: crate::tui::components::confirm_save::ConfirmSaveState::<
                jackin_config::MountConfig,
            >::new(vec![
                Line::from("Create workspace: alpha"),
                Line::from(""),
                Line::from("Working directory: /workspace"),
            ]),
        },
    )
}

pub(crate) fn modal_error_popup() -> (ManagerState<'static>, AppConfig, PathBuf) {
    with_list_modal(Modal::ErrorPopup {
        state: crate::tui::components::ErrorPopupState::new("Token mint failed", "op item missing"),
    })
}

pub(crate) fn modal_container_info() -> (ManagerState<'static>, AppConfig, PathBuf) {
    with_editor_modal(Modal::ContainerInfo {
        state: crate::tui::components::container_info_surface::ContainerInfoState::new(
            "Container",
            vec![
                crate::tui::components::container_info_surface::ContainerInfoRow::new(
                    "Run ID", "abc",
                ),
            ],
        ),
    })
}

pub(crate) fn modal_status_popup() -> (ManagerState<'static>, AppConfig, PathBuf) {
    with_list_modal(Modal::StatusPopup {
        state: crate::tui::components::StatusPopupState::new("Loading", "Resolving role"),
    })
}

pub(crate) fn modal_op_picker() -> (ManagerState<'static>, AppConfig, PathBuf) {
    with_editor_modal(Modal::OpPicker {
        secrets_target: None,
        state: Box::new(crate::tui::op_picker::OpPickerState::new()),
    })
}

pub(crate) fn modal_role_picker() -> (ManagerState<'static>, AppConfig, PathBuf) {
    with_list_modal(Modal::RolePicker {
        state: role_picker_state(),
    })
}

pub(crate) fn modal_role_override_picker() -> (ManagerState<'static>, AppConfig, PathBuf) {
    with_editor_modal(Modal::RoleOverridePicker {
        state: role_picker_state(),
    })
}

pub(crate) fn modal_source_picker() -> (ManagerState<'static>, AppConfig, PathBuf) {
    with_list_modal(Modal::SourcePicker {
        state: crate::tui::components::source_picker::SourcePickerState::new("TOKEN".into(), true),
        env_key: None,
    })
}

pub(crate) fn modal_auth_source_picker() -> (ManagerState<'static>, AppConfig, PathBuf) {
    with_editor_modal(Modal::AuthSourcePicker {
        state: crate::tui::components::source_picker::SourcePickerState::new(
            "CLAUDE_CODE_OAUTH_TOKEN".into(),
            true,
        ),
    })
}

pub(crate) fn modal_scope_picker() -> (ManagerState<'static>, AppConfig, PathBuf) {
    with_list_modal(Modal::ScopePicker {
        state: crate::tui::components::scope_picker::ScopePickerState::new(),
    })
}

pub(crate) fn modal_auth_form() -> (ManagerState<'static>, AppConfig, PathBuf) {
    let kind = crate::tui::auth::AuthKind::Claude;
    with_editor_modal(Modal::AuthForm {
        target: crate::tui::state::AuthFormTarget::Workspace { kind },
        state: Box::new(crate::tui::state::AuthForm::new(kind)),
        focus: crate::tui::state::AuthFormFocus::Mode,
        literal_buffer: String::new(),
    })
}

// ── Create-prelude wizard modal steps ──────────────────────────────────────
