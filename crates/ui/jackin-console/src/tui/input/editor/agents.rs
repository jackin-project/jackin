// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::tui::screens::editor::view::role_load_input_state;
use crate::tui::state::{EditorState, Modal, TextInputTarget};
use jackin_config::AppConfig;

pub(super) fn open_agent_override_picker(editor: &mut EditorState<'_>, config: &AppConfig) {
    use crate::tui::state::RolePickerState;
    let eligible = editor.eligible_role_override_selectors(config.roles.keys());
    if eligible.is_empty() {
        return;
    }
    editor.open_sub_modal(Modal::RoleOverridePicker {
        state: RolePickerState::new(eligible),
    });
}

pub(super) fn open_role_input(editor: &mut EditorState<'_>, config: &AppConfig) {
    let trusted_roles: Vec<String> = config
        .roles
        .iter()
        .filter(|(_, source)| source.trusted)
        .map(|(key, _)| key.clone())
        .collect();
    editor.modal = Some(Modal::TextInput {
        target: TextInputTarget::Role,
        state: role_load_input_state(trusted_roles),
    });
}

pub(super) fn toggle_agent_allowed_at_cursor(editor: &mut EditorState<'_>, config: &AppConfig) {
    let agent_names: Vec<String> = config.roles.keys().cloned().collect();
    editor.toggle_allowed_role_at_cursor(&agent_names);
}

pub(super) fn toggle_default_agent_at_cursor(editor: &mut EditorState<'_>, config: &AppConfig) {
    let agent_names: Vec<String> = config.roles.keys().cloned().collect();
    editor.toggle_default_role_at_cursor(&agent_names);
}
