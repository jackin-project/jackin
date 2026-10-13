// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn modal_focus_cases<'a>(
    config: &'a AppConfig,
    cwd: &'a std::path::Path,
) -> Vec<(&'static str, ManagerState<'a>)> {
    let mut cases: Vec<(&'static str, ManagerState<'a>)> = Vec::new();
    modal_focus_cases_list_editor(&mut cases, config, cwd);
    modal_focus_cases_settings(&mut cases, config, cwd);
    cases
}

#[expect(
    clippy::too_many_lines,
    reason = "Product-state projection table enumerates each modal and pane-tone combination"
)]
pub(super) fn modal_focus_cases_list_editor<'a>(
    cases: &mut Vec<(&'static str, ManagerState<'a>)>,
    config: &'a AppConfig,
    cwd: &'a std::path::Path,
) {
    let mut confirm_delete = ManagerState::from_config(config, cwd);
    confirm_delete.stage = ManagerStage::ConfirmDelete {
        name: "ws".to_owned(),
        state: crate::tui::components::ConfirmState::new("Delete workspace?"),
    };
    cases.push(("list confirm delete", confirm_delete));

    cases.push((
        "list confirm modal",
        list_with_modal(
            config,
            cwd,
            Modal::Confirm {
                target: crate::tui::state::ConfirmTarget::DeleteEnvVar {
                    scope: crate::tui::state::SecretsScopeTag::Workspace,
                    key: "TOKEN".into(),
                },
                state: crate::tui::components::ConfirmState::new("Delete TOKEN?"),
            },
        ),
    ));

    cases.push((
        "list save discard modal",
        list_with_modal(
            config,
            cwd,
            Modal::SaveDiscardCancel {
                state: crate::tui::components::SaveDiscardState::new("Save changes?"),
            },
        ),
    ));

    cases.push((
        "list status modal",
        list_with_modal(
            config,
            cwd,
            Modal::StatusPopup {
                state: crate::tui::components::StatusPopupState::new("Loading", "Resolving role"),
            },
        ),
    ));

    cases.push((
        "list file browser modal",
        list_with_modal(
            config,
            cwd,
            Modal::FileBrowser {
                target: crate::tui::state::FileBrowserTarget::CreateFirstMountSrc,
                state: crate::tui::components::file_browser::FileBrowserState::from_listing(
                    crate::services::file_browser::listing_at(cwd.to_path_buf(), cwd.to_path_buf()),
                ),
            },
        ),
    ));

    cases.push((
        "list mount dst choice modal",
        list_with_modal(
            config,
            cwd,
            Modal::MountDstChoice {
                target: crate::tui::state::FileBrowserTarget::CreateFirstMountSrc,
                state: crate::tui::components::mount_dst_choice::MountDstChoiceState::new(
                    "/workspace",
                ),
            },
        ),
    ));

    cases.push((
        "list workdir picker modal",
        list_with_modal(
            config,
            cwd,
            Modal::WorkdirPick {
                state: crate::tui::components::workdir_pick::WorkdirPickState::from_mounts(&[
                    jackin_config::MountConfig {
                        src: "/workspace".into(),
                        dst: "/workspace".into(),
                        readonly: false,
                        isolation: jackin_config::MountIsolation::Shared,
                    },
                ]),
            },
        ),
    ));

    cases.push((
        "list github picker modal",
        list_with_modal(
            config,
            cwd,
            Modal::GithubPicker {
                state: crate::tui::components::github_picker::GithubPickerState::new(vec![
                    crate::github_mounts::GithubChoice {
                        src: "/workspace".into(),
                        branch: "main".into(),
                        url: "https://github.com/example/repo".into(),
                    },
                ]),
            },
        ),
    ));

    cases.push((
        "list role picker modal",
        list_with_modal(
            config,
            cwd,
            Modal::RolePicker {
                state: crate::tui::state::RolePickerState::new(vec![
                    jackin_core::RoleSelector::parse("chainargos/agent-smith")
                        .expect("valid role selector"),
                ]),
            },
        ),
    ));

    cases.push((
        "list source picker modal",
        list_with_modal(
            config,
            cwd,
            Modal::SourcePicker {
                state: crate::tui::components::source_picker::SourcePickerState::new(
                    "TOKEN".into(),
                    true,
                ),
                env_key: None,
            },
        ),
    ));

    cases.push((
        "list scope picker modal",
        list_with_modal(
            config,
            cwd,
            Modal::ScopePicker {
                state: crate::tui::components::scope_picker::ScopePickerState::new(),
            },
        ),
    ));

    let mut editor_text = ManagerState::from_config(config, cwd);
    let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
    editor.set_tab_bar_focused(false);
    editor.set_tab_content_scroll_focused(true);
    editor.modal = Some(Modal::TextInput {
        target: crate::tui::state::TextInputTarget::Name,
        state: crate::tui::components::TextInputState::new("Name", "ws"),
    });
    editor_text.stage = ManagerStage::Editor(editor);
    cases.push(("editor text input", editor_text));

    let mut editor_state = ManagerState::from_config(config, cwd);
    let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
    editor.set_tab_bar_focused(false);
    editor.modal = Some(Modal::ContainerInfo {
        state: crate::tui::components::container_info_surface::ContainerInfoState::new(
            "Container",
            vec![
                crate::tui::components::container_info_surface::ContainerInfoRow::new(
                    "Run ID", "abc",
                ),
            ],
        ),
    });
    editor_state.stage = ManagerStage::Editor(editor);
    cases.push(("editor container info", editor_state));

    let mut editor_op_picker = ManagerState::from_config(config, cwd);
    let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
    editor.set_tab_bar_focused(false);
    editor.modal = Some(Modal::OpPicker {
        secrets_target: None,
        state: Box::new(crate::tui::op_picker::OpPickerState::new()),
    });
    editor_op_picker.stage = ManagerStage::Editor(editor);
    cases.push(("editor op picker", editor_op_picker));

    let mut editor_role_override = ManagerState::from_config(config, cwd);
    let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
    editor.set_tab_bar_focused(false);
    editor.modal = Some(Modal::RoleOverridePicker {
        state: crate::tui::state::RolePickerState::new(vec![
            jackin_core::RoleSelector::parse("chainargos/agent-smith")
                .expect("valid role selector"),
        ]),
    });
    editor_role_override.stage = ManagerStage::Editor(editor);
    cases.push(("editor role override picker", editor_role_override));

    let mut editor_auth_source = ManagerState::from_config(config, cwd);
    let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
    editor.set_tab_bar_focused(false);
    editor.modal = Some(Modal::AuthSourcePicker {
        state: crate::tui::components::source_picker::SourcePickerState::new(
            "CLAUDE_CODE_OAUTH_TOKEN".into(),
            true,
        ),
    });
    editor_auth_source.stage = ManagerStage::Editor(editor);
    cases.push(("editor auth source picker", editor_auth_source));

    let mut editor_auth_form = ManagerState::from_config(config, cwd);
    let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
    editor.set_tab_bar_focused(false);
    editor.modal = Some(auth_form_modal());
    editor_auth_form.stage = ManagerStage::Editor(editor);
    cases.push(("editor auth form", editor_auth_form));
}
