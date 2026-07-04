// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Editor modal leaf helpers: secret picker, token generation, and text-input commits.

use crate::tui::components::auth_panel::generated_token_op_item_name;
use crate::tui::op_picker::OpPickerState;
use crate::tui::screens::editor::update::{
    self as editor_update, EditorAuthGenerateScopePlan, editor_auth_generate_scope_plan,
};
use crate::tui::screens::editor::view::{
    secret_empty_key_label, secret_key_input_state_from_pending, secret_source_picker_state,
};
use crate::tui::state::{
    EditorState, FieldFocus, Modal, SecretsScopeTag, TextInputTarget, open_role_input_error,
};
use crate::tui::update::{CreateOpPickerPlan, create_op_picker_plan};

/// `pending_picker_target` records `(scope, Some(key))` for key rows
/// (commit replaces value) or `(scope, None)` for sentinels (commit
/// stashes path, opens `EnvKey` modal). Headers / spacers are no-ops.
pub fn open_secrets_picker_modal(
    editor: &mut EditorState<'_>,
    op_cache: std::rc::Rc<std::cell::RefCell<jackin_env::OpCache>>,
) {
    let FieldFocus::Row(n) = editor.active_field;
    let rows = editor.secrets_flat_rows();
    let Some(target) = editor_update::secret_picker_target_for_row(rows.get(n)) else {
        return;
    };
    editor.pending_picker_target = Some(target);
    editor.modal = Some(Modal::OpPicker {
        state: Box::new(OpPickerState::new_with_cache(op_cache)),
    });
}

/// Derive the [`TokenSetupScope`](jackin_env::TokenSetupScope)
/// from the auth-form generate target and the editor's Edit-mode
/// workspace name: a per-role override generates for that role, the
/// workspace form for all roles. Returns `None` when the editor is not
/// in Edit mode (Create mode has no workspace to wire yet).
fn generate_scope_for_target(
    editor: &EditorState<'_>,
    target: &crate::tui::state::AuthFormTarget,
) -> Option<jackin_env::TokenSetupScope> {
    use jackin_env::TokenSetupScope;
    editor_auth_generate_scope_plan(&editor.mode, target).map(|plan| match plan {
        EditorAuthGenerateScopePlan::Workspace(workspace) => TokenSetupScope::Workspace(workspace),
        EditorAuthGenerateScopePlan::WorkspaceRole { workspace, role } => {
            TokenSetupScope::WorkspaceRole { workspace, role }
        }
    })
}

/// Plain-text generate branch from the source picker: queue a
/// [`PendingTokenGenerate`] that mints the token. The minted literal is
/// staged into the stashed auth form (via the re-mount the loop runs on
/// completion) and persisted only when the operator Saves — the form
/// stash in `pending_auth_form_return` survives `clear_modal_chain`.
pub fn start_plain_token_generate(editor: &mut EditorState<'_>) {
    let Some(target) = editor.generating_token_target.take() else {
        super::super::auth::restore_auth_form_after_op_picker_cancel(editor);
        return;
    };
    let Some(scope) = generate_scope_for_target(editor, &target) else {
        super::super::auth::restore_auth_form_after_op_picker_cancel(editor);
        return;
    };
    editor.pending_token_generate = Some(crate::tui::state::PendingTokenGenerate {
        scope,
        args: jackin_env::TokenSetupArgs {
            plain_text: true,
            ..Default::default()
        },
    });
    editor.clear_modal_chain();
}

/// 1Password generate branch from the source picker: re-arm the target
/// and mount the Create-mode `OpPicker` so the operator chooses where the
/// freshly minted token lands (this is the pre-source-picker behaviour).
pub fn open_create_op_picker_for_generate(
    editor: &mut EditorState<'_>,
    op_cache: std::rc::Rc<std::cell::RefCell<jackin_env::OpCache>>,
) {
    let crate::tui::state::EditorMode::Edit { name } = &editor.mode else {
        editor.generating_token_target = None;
        super::super::auth::restore_auth_form_after_op_picker_cancel(editor);
        return;
    };
    let workspace_name = name.clone();
    // `generating_token_target` stays set so the OpPicker commit routes
    // back through `handle_token_generate_pick`.
    editor.modal = Some(Modal::OpPicker {
        state: Box::new(OpPickerState::new_create_with_cache(
            op_cache,
            generated_token_op_item_name(jackin_env::DEFAULT_ITEM_TEMPLATE, &workspace_name),
            jackin_env::DEFAULT_FIELD_LABEL,
        )),
    });
}

/// Translate a Create-mode `OpPicker` commit into a
/// [`PendingTokenGenerate`] request that the `run_console` loop drains
/// to mint the token. `Existing` cannot occur in Create mode; a Cancel
/// (or stray `Existing`) just closes the chain. On `Continue` the picker
/// is still drilling, so `target` is re-armed and the modal stays open.
/// The workspace name comes from `editor.mode` Edit.
pub fn handle_token_generate_pick(
    editor: &mut EditorState<'_>,
    target: crate::tui::state::AuthFormTarget,
    outcome: jackin_tui::ModalOutcome<crate::tui::op_picker::OpPickerSelection>,
) {
    use crate::tui::op_picker::OpPickerSelection;
    use jackin_env::{EditExistingTarget, TokenSetupArgs};

    let Some(scope) = generate_scope_for_target(editor, &target) else {
        super::super::auth::restore_auth_form_after_op_picker_cancel(editor);
        return;
    };

    let args = match create_op_picker_plan(outcome) {
        CreateOpPickerPlan::Commit(OpPickerSelection::NewItem {
            account,
            vault,
            item_name,
            section,
            field_label,
        }) => TokenSetupArgs {
            vault: Some(vault.id),
            item_name: Some(item_name),
            account: account.map(|a| a.id),
            reuse: None,
            field_label: Some(field_label),
            section,
            edit_existing: None,
            plain_text: false,
        },
        CreateOpPickerPlan::Commit(OpPickerSelection::EditItemField {
            account,
            vault,
            item,
            section,
            field,
        }) => TokenSetupArgs {
            vault: None,
            item_name: None,
            account: account.map(|a| a.id),
            reuse: None,
            field_label: None,
            section: None,
            edit_existing: Some(EditExistingTarget {
                vault_id: vault.id,
                item_id: item.id,
                field,
                section,
            }),
            plain_text: false,
        },
        CreateOpPickerPlan::Commit(OpPickerSelection::Existing(_)) => {
            unreachable!("create-mode OpPicker plan dismisses Existing selections")
        }
        // Still drilling — re-arm the marker the caller took and leave
        // the picker open.
        CreateOpPickerPlan::Continue => {
            editor.generating_token_target = Some(target);
            return;
        }
        // `Existing` is unreachable in Create mode; a Cancel restores
        // the stashed form. Both just close without minting.
        CreateOpPickerPlan::Dismiss => {
            super::super::auth::restore_auth_form_after_op_picker_cancel(editor);
            return;
        }
    };

    editor.pending_token_generate = Some(crate::tui::state::PendingTokenGenerate { scope, args });
    editor.clear_modal_chain();
}

/// Centralises `EnvKey` construction so every opener (Enter on
/// sentinel, A on row, P-on-sentinel fast-path, empty-key re-open)
/// stays consistent.
pub fn env_key_input_state<'a>(
    editor: &EditorState<'_>,
    scope: &SecretsScopeTag,
    label: impl Into<String>,
    initial: impl Into<String>,
) -> jackin_tui::components::TextInputState<'a> {
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
            jackin_diagnostics::debug_log!("role", "role loader input committed: raw={value:?}");
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
            // Sentinel-picker fast path: P committed an OpRef before the
            // key existed; both fields land here.
            if let Some(stashed) = editor.pending_picker_value.take() {
                set_pending_env_value_typed(editor, scope, &key, stashed);
                // env_key context now in Modal::SourcePicker
                editor.clear_modal_chain();
                return;
            }
            editor.open_sub_modal(Modal::SourcePicker {
                state: secret_source_picker_state(key.clone(), op_available),
                env_key: Some((scope.clone(), key)),
            });
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
