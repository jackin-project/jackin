//! Agent tab helpers for the editor: allow/deny toggles, override picker, role picker.

use crate::config::AppConfig;
use crate::console::tui::state::{EditorState, FieldFocus, Modal, TextInputTarget};
use jackin_console::tui::screens::editor::update as editor_update;
use jackin_console::tui::screens::editor::view::role_load_input_state;

/// Listing rules: workspace-allowed list when non-empty, otherwise
/// every role in `config.roles`. Roles already carrying an
/// override are NOT filtered out — operator may want to add more
/// keys.
pub(super) fn open_agent_override_picker(editor: &mut EditorState<'_>, config: &AppConfig) {
    use crate::selector::RolePickerState;
    use jackin_core::RoleSelector;
    let eligible: Vec<RoleSelector> = jackin_console::workspace::eligible_role_keys_for_override(
        config.roles.keys(),
        &editor.pending,
    )
    .into_iter()
    .filter_map(|name| RoleSelector::parse(&name).ok())
    .collect();
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
    crate::debug_log!(
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
    let FieldFocus::Row(n) = editor.active_field;
    // n is 0-based into config.roles (no header offset).
    let agent_names: Vec<String> = config.roles.keys().cloned().collect();
    if n >= agent_names.len() {
        return;
    }

    editor_update::toggle_allowed_role_at(
        &mut editor.pending.allowed_roles,
        &mut editor.pending.default_role,
        &agent_names,
        n,
    );
}

/// On the current default → clear; on allowed → set; on disallowed
/// → no-op (operator must `Space` to allow first).
pub(super) fn toggle_default_agent_at_cursor(editor: &mut EditorState<'_>, config: &AppConfig) {
    let FieldFocus::Row(n) = editor.active_field;
    let agent_names: Vec<String> = config.roles.keys().cloned().collect();
    if n >= agent_names.len() {
        return;
    }

    editor_update::toggle_default_role_at(
        &editor.pending.allowed_roles,
        &mut editor.pending.default_role,
        &agent_names,
        n,
    );
}
