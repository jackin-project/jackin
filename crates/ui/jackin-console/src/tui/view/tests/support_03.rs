// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
#[expect(
    clippy::too_many_lines,
    reason = "Product-state projection table enumerates each modal and pane-tone combination"
)]
pub(super) fn modal_focus_cases_settings<'a>(
    cases: &mut Vec<(&'static str, ManagerState<'a>)>,
    config: &'a AppConfig,
    cwd: &'a std::path::Path,
) {
    let mut settings_mounts_confirm = ManagerState::from_config(config, cwd);
    let mut settings = SettingsState::from_config(config);
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
            config,
            cwd,
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
            config,
            cwd,
            SettingsModal::MountFileBrowser {
                state: Box::new(
                    crate::tui::components::file_browser::FileBrowserState::from_listing(
                        crate::services::file_browser::listing_at(
                            cwd.to_path_buf(),
                            cwd.to_path_buf(),
                        ),
                    ),
                ),
            },
        ),
    ));

    cases.push((
        "settings mounts destination choice",
        settings_mounts_with_modal(
            config,
            cwd,
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
            config,
            cwd,
            SettingsModal::MountScopePicker {
                state: crate::tui::components::scope_picker::ScopePickerState::new(),
            },
        ),
    ));

    cases.push((
        "settings mounts role picker",
        settings_mounts_with_modal(
            config,
            cwd,
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
            config,
            cwd,
            SettingsModal::MountPreviewSave {
                state: crate::tui::components::confirm_save::ConfirmSaveState::new(vec![
                    ratatui::text::Line::from("Add global mount /workspace"),
                ]),
            },
        ),
    ));

    let mut settings_env_text = ManagerState::from_config(config, cwd);
    let mut settings = SettingsState::from_config(config);
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
            config,
            cwd,
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
            config,
            cwd,
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
            config,
            cwd,
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
            config,
            cwd,
            SettingsModal::EnvScopePicker {
                state: crate::tui::components::scope_picker::ScopePickerState::new(),
            },
        ),
    ));

    cases.push((
        "settings env confirm",
        settings_env_with_modal(
            config,
            cwd,
            SettingsModal::EnvConfirm {
                action: crate::tui::state::SettingsEnvConfirm::Delete,
                state: crate::tui::components::ConfirmState::new("Delete env var?"),
            },
        ),
    ));

    let mut settings_auth_text = ManagerState::from_config(config, cwd);
    let mut settings = SettingsState::from_config(config);
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
            config,
            cwd,
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
            config,
            cwd,
            SettingsModal::AuthOpPicker {
                state: Box::new(crate::tui::op_picker::OpPickerState::new()),
            },
        ),
    ));

    let kind = crate::tui::auth::AuthKind::Claude;
    cases.push((
        "settings auth form",
        settings_auth_with_modal(
            config,
            cwd,
            SettingsModal::AuthForm {
                target: crate::tui::state::AuthFormTarget::Workspace { kind },
                state: Box::new(crate::tui::state::AuthForm::new(kind)),
                focus: crate::tui::state::AuthFormFocus::Mode,
                literal_buffer: String::new(),
            },
        ),
    ));
}
