// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::tui::screens::editor::view::{
    secret_delete_confirm_state, secret_new_key_label, secret_scope_picker_state,
    secret_value_current_text, secret_value_input_state,
};
use crate::tui::state::{ConfirmTarget, EditorState, Modal, SecretsEnterPlan, TextInputTarget};

pub(super) fn open_secrets_enter_modal(editor: &mut EditorState<'_>) {
    match editor.focused_secret_enter_plan() {
        SecretsEnterPlan::EditValue { scope, key } => {
            let value = editor.secret_value(&scope, &key);
            let current =
                secret_value_current_text(value.map(jackin_core::EnvValue::as_persisted_str));
            editor.modal = Some(Modal::TextInput {
                target: TextInputTarget::EnvValue {
                    scope,
                    key: key.clone(),
                },
                state: secret_value_input_state(&key, current),
            });
        }
        SecretsEnterPlan::OpenScopePicker => {
            editor.modal = Some(Modal::ScopePicker {
                state: secret_scope_picker_state(),
            });
        }
        SecretsEnterPlan::ExpandRole(role) => {
            editor.set_secrets_role_expanded(role, true);
        }
        SecretsEnterPlan::AddRoleKey { scope } => {
            let label = secret_new_key_label(&scope);
            let state = super::env_key_input_state(editor, &scope, label, String::new());
            editor.modal = Some(Modal::TextInput {
                target: TextInputTarget::EnvKey { scope },
                state,
            });
        }
        SecretsEnterPlan::Noop => {}
    }
}

pub(super) fn open_secrets_delete_confirm(editor: &mut EditorState<'_>) {
    let Some((scope, key)) = editor.focused_secret_delete_target() else {
        return;
    };
    let state = secret_delete_confirm_state(&key);
    editor.modal = Some(Modal::Confirm {
        target: ConfirmTarget::DeleteEnvVar { scope, key },
        state,
    });
}

pub(super) fn open_secrets_add_modal(editor: &mut EditorState<'_>) {
    let Some(scope) = editor.focused_secret_add_target() else {
        return;
    };
    let label = secret_new_key_label(&scope);
    let state = super::env_key_input_state(editor, &scope, label, String::new());
    editor.modal = Some(Modal::TextInput {
        target: TextInputTarget::EnvKey { scope },
        state,
    });
}
