// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Agent tab helpers for the editor: allow/deny toggles, override picker, role picker.

use crate::tui::screens::editor::view::role_load_input_state;
use crate::tui::state::{EditorState, Modal, TextInputTarget};
use jackin_config::AppConfig;

/// Listing rules: workspace-allowed list when non-empty, otherwise
/// every role in `config.roles`. Roles already carrying an
/// override are NOT filtered out — operator may want to add more
/// keys.
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
    jackin_diagnostics::debug_log!(
        "role",
        "opening role loader input; {trusted_roles_count} trusted role(s) are blocked by the duplicate guard",
        trusted_roles_count = trusted_roles.len()
    );
    editor.modal = Some(Modal::TextInput {
        target: TextInputTarget::Role,
        state: role_load_input_state(trusted_roles),
    });
}

/// Space on an role row toggles its **effective** allow-state.
///
/// The underlying data model uses an "empty list = all allowed" shorthand,
/// so the checkbox on each row must reflect
/// `list.is_empty() || list.contains(name)`. The toggle preserves that
/// invariant in both directions:
///
/// - **Effective-allowed + empty list** (in "all" mode): populate the list
///   with every role *except* this one. Status flips to
///   `custom (total-1 of total)`; the row flips to `[ ]`.
/// - **Effective-allowed + non-empty list** (row is in the list): remove it.
///   An empty remainder is left empty (semantically = "all"); otherwise
///   stays `custom`. The row flips to `[ ]`.
/// - **Effective-blocked** (row not in list): add the name. If the list now
///   contains every role in `config.roles`, clear it back to empty
///   (= "all"). Otherwise stays `custom`. The row flips to `[x]`.
pub(super) fn toggle_agent_allowed_at_cursor(editor: &mut EditorState<'_>, config: &AppConfig) {
    let agent_names: Vec<String> = config.roles.keys().cloned().collect();
    editor.toggle_allowed_role_at_cursor(&agent_names);
}

/// On the current default → clear; on allowed → set; on disallowed
/// → no-op (operator must `Space` to allow first).
pub(super) fn toggle_default_agent_at_cursor(editor: &mut EditorState<'_>, config: &AppConfig) {
    let agent_names: Vec<String> = config.roles.keys().cloned().collect();
    editor.toggle_default_role_at_cursor(&agent_names);
}
