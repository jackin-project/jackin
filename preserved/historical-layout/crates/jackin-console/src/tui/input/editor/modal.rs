// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Editor modal leaf helpers: secret picker and text-input commits.

use crate::tui::op_picker::OpPickerState;
use crate::tui::screens::editor::update as editor_update;
use crate::tui::screens::editor::view::{
    secret_empty_key_label, secret_key_input_state_from_pending, secret_source_picker_state,
};
use crate::tui::state::{
    EditorState, FieldFocus, Modal, SecretsPickerTarget, SecretsScopeTag, TextInputTarget,
    open_role_input_error,
};

pub fn open_secrets_picker_modal(
    editor: &mut EditorState<'_>,
    op_cache: std::rc::Rc<std::cell::RefCell<jackin_env::OpCache>>,
) {
    let FieldFocus::Row(n) = editor.active_field;
    let rows = editor.secrets_flat_rows();
    let Some(target) = editor_update::secret_picker_target_for_row(rows.get(n)) else {
        return;
    };
    let secrets_target = match target {
        (scope, Some(key)) => SecretsPickerTarget::Existing { scope, key },
        (scope, None) => SecretsPickerTarget::NewKey { scope },
    };
    editor.modal = Some(Modal::OpPicker {
        secrets_target: Some(secrets_target),
        state: Box::new(OpPickerState::new_with_cache(op_cache)),
    });
}

/// Centralises `EnvKey` construction so every opener (Enter on
/// sentinel, A on row, P-on-sentinel fast-path, empty-key re-open)
/// stays consistent.
pub fn env_key_input_state<'a>(
    editor: &EditorState<'_>,
    scope: &SecretsScopeTag,
    label: impl Into<String>,
    initial: impl Into<String>,
) -> crate::tui::components::TextInputState<'a> {
    secret_key_input_state_from_pending(
        &editor.pending.env,
        &editor.pending.roles,
        scope,
        label,
        initial,
        |role| &role.env,
    )
}

/// Single source of truth for setting one env entry on the pending
/// draft. Role scope auto-creates the override entry and
/// auto-expands the section so the operator sees the new value —
/// same semantics as `ConfigEditor::set_env_var` on save.
fn set_pending_env_value(
    editor: &mut EditorState<'_>,
    scope: &SecretsScopeTag,
    key: &str,
    value: &str,
) {
    set_pending_env_value_typed(
        editor,
        scope,
        key,
        jackin_core::EnvValue::Plain(value.to_owned()),
    );
}

/// Write an `OpRef` (picker commit result) into the pending env map.
pub fn set_pending_env_op_ref(
    editor: &mut EditorState<'_>,
    scope: &SecretsScopeTag,
    key: &str,
    op_ref: jackin_core::OpRef,
) {
    set_pending_env_value_typed(editor, scope, key, jackin_core::EnvValue::OpRef(op_ref));
}

/// Write an already-typed `EnvValue` into the pending env map.
/// Used by the sentinel-add flow where the picker stashed an `OpRef`
/// before the key name was known.
fn set_pending_env_value_typed(
    editor: &mut EditorState<'_>,
    scope: &SecretsScopeTag,
    key: &str,
    value: jackin_core::EnvValue,
) {
    editor_update::set_secret_value(
        &mut editor.pending.env,
        &mut editor.pending.roles,
        &mut editor.secrets_expanded,
        scope,
        key,
        value,
        |roles, role| {
            roles.entry(role.to_owned()).or_default();
        },
        |role| &mut role.env,
    );
}

pub fn apply_text_input_to_pending(
    target: &TextInputTarget,
    editor: &mut EditorState<'_>,
    value: &str,
    op_available: bool,
) {
    match target {
        TextInputTarget::Name => {
            editor.commit_workspace_name_input(value);
        }
        TextInputTarget::Workdir => {
            editor.commit_workdir_input(value);
        }
        TextInputTarget::MountDst => {
            editor.commit_last_mount_dst_input(value);
        }
        TextInputTarget::Role => {
            open_role_input_error(
                editor,
                crate::tui::components::error_popup::role_input_misroute_error_message(),
            );
        }
        TextInputTarget::EnvKey { scope } => {
            // Empty key re-opens the EnvKey modal with the inline
            // "cannot be empty" label instead of committing.
            let trimmed = value.trim();
            if trimmed.is_empty() {
                // env_key context now in Modal::SourcePicker
                let state =
                    env_key_input_state(editor, scope, secret_empty_key_label(), String::new());
                editor.modal = Some(Modal::TextInput {
                    target: TextInputTarget::EnvKey {
                        scope: scope.clone(),
                    },
                    state,
                });
                return;
            }
            let key = trimmed.to_owned();
            editor.open_sub_modal(Modal::SourcePicker {
                state: secret_source_picker_state(key.clone(), op_available),
                env_key: Some((scope.clone(), key)),
            });
        }
        TextInputTarget::EnvKeyWithValue {
            scope,
            value: carried_value,
        } => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                let state =
                    env_key_input_state(editor, scope, secret_empty_key_label(), String::new());
                editor.modal = Some(Modal::TextInput {
                    target: TextInputTarget::EnvKeyWithValue {
                        scope: scope.clone(),
                        value: carried_value.clone(),
                    },
                    state,
                });
                return;
            }
            set_pending_env_value_typed(editor, scope, trimmed, carried_value.clone());
            editor.clear_modal_chain();
        }
        TextInputTarget::EnvValue { scope, key } => {
            set_pending_env_value(editor, scope, key, value);
            editor.clear_modal_chain();
        }
        TextInputTarget::AuthCredential => {
            super::super::auth::apply_plain_text_to_auth_form(editor, value);
        }
    }
}
