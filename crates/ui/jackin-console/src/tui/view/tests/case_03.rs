// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "Product-state projection table enumerates each modal and pane-tone combination"
)]
fn host_console_modal_states_project_visible_focus() {
    let config = AppConfig::default();
    let cwd = test_cwd();
    let mut cases: Vec<(&str, ManagerState<'_>)> = Vec::new();

    let mut confirm_delete = ManagerState::from_config(&config, &cwd);
    confirm_delete.stage = ManagerStage::ConfirmDelete {
        name: "ws".to_owned(),
        state: crate::tui::components::ConfirmState::new("Delete workspace?"),
    };
    cases.push(("list confirm delete", confirm_delete));

    cases.push((
        "list confirm modal",
        list_with_modal(
            &config,
            &cwd,
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
            &config,
            &cwd,
            Modal::SaveDiscardCancel {
                state: crate::tui::components::SaveDiscardState::new("Save changes?"),
            },
        ),
    ));

    cases.push((
        "list status modal",
        list_with_modal(
            &config,
            &cwd,
            Modal::StatusPopup {
                state: crate::tui::components::StatusPopupState::new("Loading", "Resolving role"),
            },
        ),
    ));

    cases.push((
        "list file browser modal",
        list_with_modal(
            &config,
            &cwd,
            Modal::FileBrowser {
                target: crate::tui::state::FileBrowserTarget::CreateFirstMountSrc,
                state: crate::tui::components::file_browser::FileBrowserState::from_listing(
                    crate::services::file_browser::listing_at(cwd.clone(), cwd.clone()),
                ),
            },
        ),
    ));

    cases.push((
        "list mount dst choice modal",
        list_with_modal(
            &config,
            &cwd,
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
            &config,
            &cwd,
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
            &config,
            &cwd,
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
            &config,
            &cwd,
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
            &config,
            &cwd,
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
            &config,
            &cwd,
            Modal::ScopePicker {
                state: crate::tui::components::scope_picker::ScopePickerState::new(),
            },
        ),
    ));

    let mut editor_text = ManagerState::from_config(&config, &cwd);
    let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
    editor.set_tab_bar_focused(false);
    editor.set_tab_content_scroll_focused(true);
    editor.modal = Some(Modal::TextInput {
        target: crate::tui::state::TextInputTarget::Name,
        state: crate::tui::components::TextInputState::new("Name", "ws"),
    });
    editor_text.stage = ManagerStage::Editor(editor);
    cases.push(("editor text input", editor_text));

    let mut editor_state = ManagerState::from_config(&config, &cwd);
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

    let mut editor_op_picker = ManagerState::from_config(&config, &cwd);
    let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
    editor.set_tab_bar_focused(false);
    editor.modal = Some(Modal::OpPicker {
        secrets_target: None,
        state: Box::new(crate::tui::op_picker::OpPickerState::new()),
    });
    editor_op_picker.stage = ManagerStage::Editor(editor);
    cases.push(("editor op picker", editor_op_picker));

    let mut editor_role_override = ManagerState::from_config(&config, &cwd);
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

    let mut editor_auth_source = ManagerState::from_config(&config, &cwd);
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

    let mut editor_auth_form = ManagerState::from_config(&config, &cwd);
    let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
    editor.set_tab_bar_focused(false);
    editor.modal = Some(auth_form_modal());
    editor_auth_form.stage = ManagerStage::Editor(editor);
    cases.push(("editor auth form", editor_auth_form));

    let mut settings_mounts_confirm = ManagerState::from_config(&config, &cwd);
    let mut settings = SettingsState::from_config(&config);
    settings.active_tab = crate::tui::state::SettingsTab::Mounts;
    settings.set_active_content_focused(true);
    settings.mounts.modals.open(SettingsModal::MountConfirm {
        action: GlobalMountConfirm::Remove,
        state: crate::tui::components::ConfirmState::new("Remove mount?"),
    });
    settings_mounts_confirm.stage = ManagerStage::Settings(settings);
    cases.push(("settings mounts confirm", settings_mounts_confirm));

    cases.push((
        "settings mounts text",
        settings_mounts_with_modal(
            &config,
            &cwd,
            SettingsModal::MountText {
                target: crate::tui::state::GlobalMountTextTarget::AddName,
                state: Box::new(crate::tui::components::TextInputState::new(
                    "Mount name",
                    "repo",
                )),
            },
        ),
    ));

    cases.push((
        "settings mounts file browser",
        settings_mounts_with_modal(
            &config,
            &cwd,
            SettingsModal::MountFileBrowser {
                state: Box::new(
                    crate::tui::components::file_browser::FileBrowserState::from_listing(
                        crate::services::file_browser::listing_at(cwd.clone(), cwd.clone()),
                    ),
                ),
            },
        ),
    ));

    cases.push((
        "settings mounts destination choice",
        settings_mounts_with_modal(
            &config,
            &cwd,
            SettingsModal::MountDstChoice {
                state: crate::tui::components::mount_dst_choice::MountDstChoiceState::new(
                    "/workspace",
                ),
            },
        ),
    ));

    cases.push((
        "settings mounts scope picker",
        settings_mounts_with_modal(
            &config,
            &cwd,
            SettingsModal::MountScopePicker {
                state: crate::tui::components::scope_picker::ScopePickerState::new(),
            },
        ),
    ));

    cases.push((
        "settings mounts role picker",
        settings_mounts_with_modal(
            &config,
            &cwd,
            SettingsModal::MountRolePicker {
                state: crate::tui::state::RolePickerState::new(vec![
                    jackin_core::RoleSelector::parse("chainargos/agent-smith")
                        .expect("valid role selector"),
                ]),
            },
        ),
    ));

    cases.push((
        "settings mounts preview save",
        settings_mounts_with_modal(
            &config,
            &cwd,
            SettingsModal::MountPreviewSave {
                state: crate::tui::components::confirm_save::ConfirmSaveState::new(vec![
                    ratatui::text::Line::from("Add global mount /workspace"),
                ]),
            },
        ),
    ));

    let mut settings_env_text = ManagerState::from_config(&config, &cwd);
    let mut settings = SettingsState::from_config(&config);
    settings.active_tab = crate::tui::state::SettingsTab::Environments;
    settings.set_active_content_focused(true);
    settings.env.modals.open(SettingsModal::EnvText {
        target: SettingsEnvTextTarget::EnvKey {
            scope: SettingsEnvScope::Global,
        },
        pending_value: None,
        state: Box::new(crate::tui::components::TextInputState::new(
            "Environment key",
            "TOKEN",
        )),
    });
    settings_env_text.stage = ManagerStage::Settings(settings);
    cases.push(("settings env text", settings_env_text));

    cases.push((
        "settings env source picker",
        settings_env_with_modal(
            &config,
            &cwd,
            SettingsModal::EnvSourcePicker {
                key: (SettingsEnvScope::Global, "TOKEN".to_owned()),
                state: crate::tui::components::source_picker::SourcePickerState::new(
                    "TOKEN".into(),
                    true,
                ),
            },
        ),
    ));

    cases.push((
        "settings env op picker",
        settings_env_with_modal(
            &config,
            &cwd,
            SettingsModal::EnvOpPicker {
                target: crate::tui::state::SettingsEnvOpPickerTarget::Existing {
                    scope: SettingsEnvScope::Global,
                    key: "TOKEN".to_owned(),
                },
                state: Box::new(crate::tui::op_picker::OpPickerState::new()),
            },
        ),
    ));

    cases.push((
        "settings env role picker",
        settings_env_with_modal(
            &config,
            &cwd,
            SettingsModal::EnvRolePicker {
                state: crate::tui::state::RolePickerState::new(vec![
                    jackin_core::RoleSelector::parse("chainargos/agent-smith")
                        .expect("valid role selector"),
                ]),
            },
        ),
    ));

    cases.push((
        "settings env scope picker",
        settings_env_with_modal(
            &config,
            &cwd,
            SettingsModal::EnvScopePicker {
                state: crate::tui::components::scope_picker::ScopePickerState::new(),
            },
        ),
    ));

    cases.push((
        "settings env confirm",
        settings_env_with_modal(
            &config,
            &cwd,
            SettingsModal::EnvConfirm {
                action: crate::tui::state::SettingsEnvConfirm::Delete,
                state: crate::tui::components::ConfirmState::new("Delete env var?"),
            },
        ),
    ));

    let mut settings_auth_text = ManagerState::from_config(&config, &cwd);
    let mut settings = SettingsState::from_config(&config);
    settings.active_tab = crate::tui::state::SettingsTab::Auth;
    settings.set_active_content_focused(true);
    settings.auth.modals.open(SettingsModal::AuthTextInput {
        state: Box::new(crate::tui::components::TextInputState::new(
            "Credential",
            "token",
        )),
    });
    settings_auth_text.stage = ManagerStage::Settings(settings);
    cases.push(("settings auth text", settings_auth_text));

    cases.push((
        "settings auth source picker",
        settings_auth_with_modal(
            &config,
            &cwd,
            SettingsModal::AuthSourcePicker {
                state: crate::tui::components::source_picker::SourcePickerState::new(
                    "CLAUDE_CODE_OAUTH_TOKEN".into(),
                    true,
                ),
            },
        ),
    ));

    cases.push((
        "settings auth op picker",
        settings_auth_with_modal(
            &config,
            &cwd,
            SettingsModal::AuthOpPicker {
                state: Box::new(crate::tui::op_picker::OpPickerState::new()),
            },
        ),
    ));

    let kind = crate::tui::auth::AuthKind::Claude;
    cases.push((
        "settings auth form",
        settings_auth_with_modal(
            &config,
            &cwd,
            SettingsModal::AuthForm {
                target: crate::tui::state::AuthFormTarget::Workspace { kind },
                state: Box::new(crate::tui::state::AuthForm::new(kind)),
                focus: crate::tui::state::AuthFormFocus::Mode,
                literal_buffer: String::new(),
            },
        ),
    ));

    for (name, mut state) in cases {
        let buf = render_manager_buffer(&mut state, &config, &cwd, 100, 28);
        assert!(
            focused_region_count(&buf) >= 1,
            "{name} must project focused state into the rendered composition"
        );
    }
}

