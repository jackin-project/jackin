// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Stage and screen baseline builders.
#![cfg(test)]

use super::*;
use std::path::PathBuf;

use crate::tui::state::{EditorState, EditorTab, ManagerStage, ManagerState, SettingsState};
use jackin_config::{AppConfig, WorkspaceConfig};

pub(crate) fn workspaces_list_empty() -> (ManagerState<'static>, AppConfig, PathBuf) {
    plain()
}

pub(crate) fn workspaces_list_populated() -> (ManagerState<'static>, AppConfig, PathBuf) {
    let config = populated_config();
    let cwd = test_cwd();
    let state = ManagerState::from_config(&config, &cwd);
    (state, config, cwd)
}

pub(crate) fn editor_with_tab(tab: EditorTab) -> (ManagerState<'static>, AppConfig, PathBuf) {
    let config = populated_config();
    let cwd = test_cwd();
    let mut state = ManagerState::from_config(&config, &cwd);
    let mut editor = EditorState::new_edit("alpha".into(), WorkspaceConfig::default());
    editor.active_tab = tab;
    state.stage = ManagerStage::Editor(editor);
    (state, config, cwd)
}

pub(crate) fn editor_general() -> (ManagerState<'static>, AppConfig, PathBuf) {
    editor_with_tab(EditorTab::General)
}

pub(crate) fn editor_mounts() -> (ManagerState<'static>, AppConfig, PathBuf) {
    editor_with_tab(EditorTab::Mounts)
}

pub(crate) fn editor_roles() -> (ManagerState<'static>, AppConfig, PathBuf) {
    editor_with_tab(EditorTab::Roles)
}

pub(crate) fn editor_secrets() -> (ManagerState<'static>, AppConfig, PathBuf) {
    editor_with_tab(EditorTab::Secrets)
}

pub(crate) fn editor_auth() -> (ManagerState<'static>, AppConfig, PathBuf) {
    editor_with_tab(EditorTab::Auth)
}

pub(crate) fn settings_with_tab(
    tab: crate::tui::state::SettingsTab,
) -> (ManagerState<'static>, AppConfig, PathBuf) {
    let config = populated_config();
    let cwd = test_cwd();
    let mut state = ManagerState::from_config(&config, &cwd);
    let mut settings = SettingsState::from_config(&config);
    settings.active_tab = tab;
    state.stage = ManagerStage::Settings(settings);
    (state, config, cwd)
}

pub(crate) fn settings_general() -> (ManagerState<'static>, AppConfig, PathBuf) {
    settings_with_tab(crate::tui::state::SettingsTab::General)
}

pub(crate) fn settings_mounts() -> (ManagerState<'static>, AppConfig, PathBuf) {
    settings_with_tab(crate::tui::state::SettingsTab::Mounts)
}

pub(crate) fn settings_environments() -> (ManagerState<'static>, AppConfig, PathBuf) {
    settings_with_tab(crate::tui::state::SettingsTab::Environments)
}

pub(crate) fn settings_auth() -> (ManagerState<'static>, AppConfig, PathBuf) {
    settings_with_tab(crate::tui::state::SettingsTab::Auth)
}

pub(crate) fn settings_trust() -> (ManagerState<'static>, AppConfig, PathBuf) {
    settings_with_tab(crate::tui::state::SettingsTab::Trust)
}

pub(crate) fn create_prelude() -> (ManagerState<'static>, AppConfig, PathBuf) {
    let (mut state, config, cwd) = plain();
    state.stage = ManagerStage::CreatePrelude(crate::tui::state::CreatePreludeState::default());
    (state, config, cwd)
}

pub(crate) fn confirm_delete() -> (ManagerState<'static>, AppConfig, PathBuf) {
    let (mut state, config, cwd) = populated_then();
    state.stage = ManagerStage::ConfirmDelete {
        name: "alpha".to_owned(),
        state: crate::tui::components::ConfirmState::new("Delete workspace?"),
    };
    (state, config, cwd)
}

pub(crate) fn populated_then() -> (ManagerState<'static>, AppConfig, PathBuf) {
    let config = populated_config();
    let cwd = test_cwd();
    let state = ManagerState::from_config(&config, &cwd);
    (state, config, cwd)
}

pub(crate) fn confirm_instance_purge() -> (ManagerState<'static>, AppConfig, PathBuf) {
    let (mut state, config, cwd) = populated_then();
    state.stage = ManagerStage::ConfirmInstancePurge {
        container: "jackin-alpha".to_owned(),
        label: "alpha".to_owned(),
        state: crate::tui::components::ConfirmState::new(
            "Purge instance?\nThis removes the container and its state.",
        ),
    };
    (state, config, cwd)
}

pub(crate) fn keyboard_help() -> (ManagerState<'static>, AppConfig, PathBuf) {
    let (mut state, config, cwd) = populated_then();
    state.keyboard_help = Some(termrock::widgets::KeyboardHelpState::modal());
    (state, config, cwd)
}

// ── Modal constructors (all 18 `ConsoleModal` variants) ────────────────────
