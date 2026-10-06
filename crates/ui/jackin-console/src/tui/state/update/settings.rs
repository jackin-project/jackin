// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Settings selection moves and toggles.

use crate::tui::screens::settings::update::settings_tab_move_plan;

use super::super::{ManagerStage, ManagerState, SecretsScopeTag};

pub(crate) fn move_settings_tab(state: &mut ManagerState<'_>, delta: isize, focus_tab_bar: bool) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    let plan = settings_tab_move_plan(settings.active_tab, delta, focus_tab_bar);
    settings.apply_tab_move_plan(plan);
}

pub(crate) fn move_settings_general_selection(state: &mut ManagerState<'_>, delta: isize) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    settings.general.move_selection(delta);
}

pub(crate) fn toggle_settings_general_selected(state: &mut ManagerState<'_>) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    settings.general.toggle_selected();
}

pub(crate) fn set_editor_secrets_role_expanded(
    state: &mut ManagerState<'_>,
    role: String,
    expanded: bool,
) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    editor.set_secrets_role_expanded(role, expanded);
}

pub(crate) fn toggle_editor_general_selected(state: &mut ManagerState<'_>) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    editor.toggle_general_selected();
}

pub(crate) fn toggle_editor_mount_readonly_selected(state: &mut ManagerState<'_>) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    editor.toggle_selected_mount_readonly();
}

pub(crate) fn toggle_editor_secret_mask(
    state: &mut ManagerState<'_>,
    scope: SecretsScopeTag,
    key: String,
) {
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return;
    };
    editor.toggle_secret_mask(scope, key);
}

pub(crate) fn set_settings_env_role_expanded(
    state: &mut ManagerState<'_>,
    role: String,
    expanded: bool,
) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    settings.env.set_role_expanded(role, expanded);
}

pub(crate) fn toggle_settings_global_mount_readonly(state: &mut ManagerState<'_>) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    settings.mounts.toggle_selected_readonly();
}

pub(crate) fn toggle_settings_trust_selected(state: &mut ManagerState<'_>) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    settings.trust.toggle_selected();
}

pub(crate) fn move_settings_auth_selection(state: &mut ManagerState<'_>, delta: isize) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    settings.auth.move_selection(delta);
}
