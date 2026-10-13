// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn list_file_browser_hints_reach_reserved_footer() {
    let config = AppConfig::default();
    let cwd = std::env::current_dir().unwrap();
    let mut state = ManagerState::from_config(&config, &cwd);
    state.list_modal = Some(Modal::FileBrowser {
        target: FileBrowserTarget::EditAddMountSrc,
        state: file_browser_state(),
    });

    assert_file_browser_hints(workspace_screen_footer_items_for_state(
        &state,
        &config,
        &cwd,
        Rect::new(0, 0, 120, 40),
    ));
}

#[test]
fn create_prelude_file_browser_hints_reach_reserved_footer() {
    let config = AppConfig::default();
    let cwd = std::env::current_dir().unwrap();
    let mut state = ManagerState::from_config(&config, &cwd);
    let mut prelude = CreatePreludeState::new();
    prelude.modal = Some(Modal::FileBrowser {
        target: FileBrowserTarget::CreateFirstMountSrc,
        state: file_browser_state(),
    });
    state.stage = ConsoleManagerStage::CreatePrelude(prelude);

    assert_file_browser_hints(workspace_screen_footer_items_for_state(
        &state,
        &config,
        &cwd,
        Rect::new(0, 0, 120, 40),
    ));
}

#[test]
fn editor_file_browser_hints_reach_footer() {
    let config = AppConfig::default();
    let mut editor = crate::tui::state::EditorState::new_edit(
        "workspace".to_owned(),
        WorkspaceConfig::default(),
    );
    editor.modal = Some(Modal::FileBrowser {
        target: FileBrowserTarget::EditAddMountSrc,
        state: file_browser_state(),
    });

    assert_file_browser_hints(editor_footer_items(
        &editor,
        &config,
        false,
        Rect::new(0, 0, 120, 40),
    ));
}

#[test]
fn settings_mounts_file_browser_hints_reach_footer() {
    let config = AppConfig::default();
    let mut settings = SettingsState::from_config(&config);
    settings.active_tab = SettingsTab::Mounts;
    settings
        .mounts
        .modals
        .open(SettingsModal::MountFileBrowser {
            state: Box::new(file_browser_state()),
        });

    assert_file_browser_hints(settings_screen_footer_for_state(
        &settings,
        false,
        Rect::new(0, 0, 120, 40),
    ));
}

#[test]
fn settings_auth_file_browser_hints_reach_footer() {
    let config = AppConfig::default();
    let mut settings = SettingsState::from_config(&config);
    settings.active_tab = SettingsTab::Auth;
    settings
        .auth
        .modals
        .open(SettingsModal::AuthSourceFolderPicker {
            state: file_browser_state(),
        });

    assert_file_browser_hints(settings_screen_footer_for_state(
        &settings,
        false,
        Rect::new(0, 0, 120, 40),
    ));
}
