// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Editor modal key handling.

use super::{EditorModalOutcome, env_key_input_state};
use crate::tui::screens::editor::view::{
    secret_new_key_after_picker_label, secret_new_key_label, secret_new_value_input_state,
};
use crate::tui::state::{
    ConfirmTarget, EditorSaveFlow, EditorState, ExitIntent, FileBrowserTarget, Modal,
    SecretsPickerTarget, SecretsScopeTag, TextInputTarget, open_editor_action_error,
};
use crate::tui::update::{
    BoolConfirmModalPlan, ConfirmSaveModalPlan, DismissibleModalPlan, FileBrowserModalPlan,
    InlinePickerPlan, SaveDiscardModalPlan, ScopePickerPlan, SourcePickerPlan,
    bool_confirm_modal_plan, confirm_save_modal_plan, dismissible_modal_plan,
    file_browser_modal_plan, inline_picker_plan, save_discard_modal_plan, scope_picker_plan,
    source_picker_plan,
};
#[expect(
    clippy::too_many_lines,
    reason = "Editor-modal input dispatcher handling every per-modal-state key \
              binding inline. Each key-event arm carries its own focused state \
              transition; extracting arms into sub-dispatchers would require \
              re-borrowing the editor state across fn boundaries and obscure \
              the per-binding readability."
)]
pub fn handle_editor_modal(
    editor: &mut EditorState<'_>,
    key: crossterm::event::KeyEvent,
    op_available: bool,
    op_cache: std::rc::Rc<std::cell::RefCell<jackin_env::OpCache>>,
    config: &mut jackin_config::AppConfig,
    _paths: &jackin_core::JackinPaths,
    term_size: ratatui::layout::Rect,
) -> EditorModalOutcome {
    let Some(modal) = editor.modal.as_mut() else {
        return EditorModalOutcome::Continue;
    };
    match modal {
        Modal::TextInput { target, state } => {
            match inline_picker_plan(state.handle_key(key.into())) {
                InlinePickerPlan::Commit(value) => {
                    let target = target.clone();
                    if target == TextInputTarget::Role {
                        editor.clear_modal_chain();
                        return super::apply_role_input(editor, config, &value);
                    }
                    super::apply_text_input_to_pending(&target, editor, &value, op_available);
                }
                InlinePickerPlan::Dismiss => {
                    let target = target.clone();
                    if matches!(target, TextInputTarget::AuthCredential) {
                        // Plain-text leg of the source-picker round trip
                        // recovers identically to the OpPicker leg.
                        editor.dismiss_active_modal();
                        super::super::auth::restore_auth_form_after_op_picker_cancel(editor);
                        return EditorModalOutcome::Continue;
                    }
                    editor.pop_modal_chain();
                }
                InlinePickerPlan::Continue => {}
            }
        }
        Modal::FileBrowser { state, .. } => {
            let page_rows =
                crate::tui::components::file_browser::page_rows_for_modal(term_size, state);
            let outcome = state.handle_key_with_page_rows(key, Some(page_rows));
            match file_browser_modal_plan(outcome) {
                FileBrowserModalPlan::Dismiss => {
                    editor.pop_modal_chain();
                }
                FileBrowserModalPlan::ResolveGitUrl(path) => {
                    return EditorModalOutcome::ResolveFileBrowserGitUrl(path);
                }
                FileBrowserModalPlan::OpenUrl(url) => return EditorModalOutcome::OpenUrl(url),
                FileBrowserModalPlan::Continue => {}
                FileBrowserModalPlan::ApplyFileBrowserOutcome(outcome) => {
                    return EditorModalOutcome::ApplyFileBrowserOutcome(outcome);
                }
            }
        }
        Modal::WorkdirPick { state } => match inline_picker_plan(state.handle_key(key)) {
            InlinePickerPlan::Commit(workdir) => {
                editor.commit_workdir_input(workdir);
            }
            InlinePickerPlan::Dismiss => {
                editor.pop_modal_chain();
            }
            InlinePickerPlan::Continue => {}
        },
        Modal::Confirm { target, state } => {
            match bool_confirm_modal_plan(state.handle_key(key.into())) {
                BoolConfirmModalPlan::Confirm => {
                    let target = target.clone();
                    editor.clear_modal_chain();
                    // Source-drift acknowledgement consumes `plan` and
                    // re-stashes it as a `PendingCommit` for the outer
                    // dispatcher (which owns `paths` / `cwd` / `runner`)
                    // to drain via `commit_editor_save`.
                    if let ConfirmTarget::DeleteIsolatedAndSave {
                        mut plan,
                        exit_on_success,
                        ..
                    } = target
                    {
                        plan.delete_isolated_acknowledged = true;
                        plan.isolated_cleanup_complete = false;
                        editor.save_flow = EditorSaveFlow::PendingCommit {
                            plan,
                            exit_on_success,
                        };
                    } else {
                        match super::apply_editor_confirm(editor, &target) {
                            Ok(EditorModalOutcome::Continue) => {}
                            Ok(outcome) => return outcome,
                            Err(e) => open_editor_action_error(editor, &e),
                        }
                    }
                }
                BoolConfirmModalPlan::Dismiss => {
                    let was_drift = matches!(target, ConfirmTarget::DeleteIsolatedAndSave { .. });
                    editor.clear_modal_chain();
                    if was_drift {
                        editor.save_flow = EditorSaveFlow::Idle;
                    }
                }
                BoolConfirmModalPlan::Continue => {}
            }
        }
        Modal::MountDstChoice {
            target,
            state: modal_state,
        } => {
            let target = target.clone();
            let src = modal_state.src.clone();
            let outcome = modal_state.handle_key(key);
            super::dispatch_editor_mount_dst_choice(editor, target, &src, &outcome);
        }
        Modal::SaveDiscardCancel { state: modal_state } => {
            match save_discard_modal_plan(modal_state.handle_key(key.into())) {
                SaveDiscardModalPlan::Save => {
                    editor.clear_modal_chain();
                    editor.exit_after_save = Some(ExitIntent::Save);
                }
                SaveDiscardModalPlan::Discard => {
                    editor.clear_modal_chain();
                    editor.exit_after_save = Some(ExitIntent::Discard);
                }
                SaveDiscardModalPlan::Dismiss => {
                    editor.clear_modal_chain();
                }
                SaveDiscardModalPlan::Continue => {}
            }
        }
        // List-view modals; defensive cancel if one lands here.
        Modal::GithubPicker { .. } | Modal::RolePicker { .. } => {
            editor.clear_modal_chain();
        }
        Modal::RoleOverridePicker { state: picker } => {
            match inline_picker_plan(picker.handle_key(key)) {
                InlinePickerPlan::Commit(role) => {
                    // The override section materializes organically on
                    // the first value commit; we don't touch
                    // `pending.roles` here, so a cancel mid-flow leaves
                    // no empty placeholder.
                    let role_name = role.key();
                    let scope = SecretsScopeTag::Role(role_name);
                    let label = secret_new_key_label(&scope);
                    let state = env_key_input_state(editor, &scope, label, "");
                    editor.open_sub_modal(Modal::TextInput {
                        target: TextInputTarget::EnvKey { scope },
                        state,
                    });
                }
                InlinePickerPlan::Dismiss => {
                    editor.pop_modal_chain();
                }
                InlinePickerPlan::Continue => {}
            }
        }
        Modal::ConfirmSave { state: modal_state } => {
            match confirm_save_modal_plan(modal_state.handle_key(key)) {
                ConfirmSaveModalPlan::Commit => {
                    // Confirming → PendingCommit atomically so plan +
                    // exit_on_success travel together to the outer
                    // handler that holds paths/cwd.
                    let plan = crate::tui::state::PendingSaveCommit {
                        effective_removals: modal_state.effective_removals.clone(),
                        final_mounts: modal_state.final_mounts.clone(),
                        // First commit pass — the drift check in
                        // `commit_editor_save` runs unconditionally. The
                        // `DeleteIsolatedAndSave` confirm modal is what
                        // re-stashes the plan with the flag flipped to
                        // `true` so the second pass skips the check.
                        delete_isolated_acknowledged: false,
                        isolated_cleanup_complete: false,
                    };
                    let exit_on_success = matches!(
                        editor.save_flow,
                        EditorSaveFlow::Confirming {
                            exit_on_success: true
                        }
                    );
                    editor.clear_modal_chain();
                    editor.save_flow = EditorSaveFlow::PendingCommit {
                        plan,
                        exit_on_success,
                    };
                }
                ConfirmSaveModalPlan::Dismiss => {
                    editor.clear_modal_chain();
                    editor.save_flow = EditorSaveFlow::Idle;
                }
                ConfirmSaveModalPlan::Continue => {}
            }
        }
        Modal::ErrorPopup { state: popup_state } => {
            match dismissible_modal_plan(popup_state.handle_key(key.into())) {
                DismissibleModalPlan::Dismiss => {
                    // A source-folder validation rejection stacks this popup
                    // directly over the auth source-folder picker. Dismissing it
                    // returns to that picker so the operator can pick another
                    // folder, rather than tearing down the whole auth flow.
                    if matches!(
                        editor.modal_parents.last(),
                        Some(Modal::FileBrowser {
                            target: FileBrowserTarget::AuthFormSourceFolder,
                            ..
                        })
                    ) {
                        editor.pop_modal_chain();
                        return EditorModalOutcome::Continue;
                    }
                    editor.clear_modal_chain();
                    editor.save_flow = EditorSaveFlow::Idle;
                    // If the popup was raised by a failed OpPicker commit
                    // for the auth form, the form's state was re-stashed
                    // into the modal parent stack instead of being
                    // re-mounted directly — restore it now so the operator
                    // lands back on the form with the prior credential
                    // unchanged, ready to retry through the source picker.
                    if editor.has_modal_parent() {
                        super::super::auth::restore_auth_form_after_op_picker_cancel(editor);
                    }
                }
                DismissibleModalPlan::Continue => {}
            }
        }
        Modal::StatusPopup { .. } | Modal::ContainerInfo { .. } => {}
        Modal::ScopePicker { state: scope_state } => {
            match scope_picker_plan(scope_state.handle_key(key)) {
                ScopePickerPlan::AllAgents => {
                    let scope = SecretsScopeTag::Workspace;
                    let state =
                        env_key_input_state(editor, &scope, secret_new_key_label(&scope), "");
                    editor.open_sub_modal(Modal::TextInput {
                        target: TextInputTarget::EnvKey { scope },
                        state,
                    });
                }
                ScopePickerPlan::SpecificAgent => {
                    // Empty eligible set → `open_agent_override_picker`
                    // is a no-op; we close the modal then.
                    super::agents::open_agent_override_picker(editor, config);
                    if !editor.has_active_role_override_picker() {
                        editor.clear_modal_chain();
                    }
                }
                ScopePickerPlan::Dismiss => {
                    editor.pop_modal_chain();
                }
                ScopePickerPlan::Continue => {}
            }
        }
        Modal::SourcePicker {
            state: source,
            env_key,
        } => {
            match source_picker_plan(source.handle_key(key)) {
                SourcePickerPlan::Plain => {
                    let Some((scope, key)) = env_key.take() else {
                        editor.clear_modal_chain();
                        return EditorModalOutcome::Continue;
                    };
                    editor.open_sub_modal(Modal::TextInput {
                        target: TextInputTarget::EnvValue {
                            scope,
                            key: key.clone(),
                        },
                        state: secret_new_value_input_state(&key),
                    });
                }
                SourcePickerPlan::Op => {
                    let Some((scope, key)) = env_key.take() else {
                        editor.clear_modal_chain();
                        return EditorModalOutcome::Continue;
                    };
                    editor.open_sub_modal(Modal::OpPicker {
                        secrets_target: Some(SecretsPickerTarget::Existing { scope, key }),
                        state: Box::new(crate::tui::op_picker::OpPickerState::new_with_cache(
                            op_cache,
                        )),
                    });
                }
                SourcePickerPlan::Dismiss => {
                    // Cancel: drop the in-flight key name and close
                    // the modal. Operator returns to the Secrets tab
                    // with no env entry added.
                    editor.pop_modal_chain();
                }
                SourcePickerPlan::Continue => {}
            }
        }
        Modal::AuthSourcePicker { state: source } => {
            let outcome = source.handle_key(key);
            match source_picker_plan(outcome) {
                SourcePickerPlan::Plain => {
                    super::super::auth::apply_plain_source_picker_to_auth_form(editor);
                }
                SourcePickerPlan::Op => {
                    super::super::auth::open_op_picker_from_auth_source(editor, op_cache);
                }
                SourcePickerPlan::Dismiss => {
                    super::super::auth::restore_auth_form_after_op_picker_cancel(editor);
                }
                SourcePickerPlan::Continue => {}
            }
        }
        Modal::AuthForm { .. } => {
            let _ = super::super::auth::handle_auth_form_key(editor, key, op_available);
        }
        Modal::OpPicker {
            secrets_target,
            state: picker,
        } => {
            let outcome = picker.handle_key(key);
            let secrets_target = secrets_target.clone();
            match crate::tui::update::op_picker_inline_plan(outcome) {
                // Browse-mode caller: only `Existing` is reachable.
                InlinePickerPlan::Commit(
                    crate::tui::op_picker::OpPickerSelection::NewItem { .. }
                    | crate::tui::op_picker::OpPickerSelection::EditItemField { .. },
                ) => unreachable!("Secrets-tab OpPicker runs in Browse mode"),
                InlinePickerPlan::Commit(crate::tui::op_picker::OpPickerSelection::Existing(
                    op_ref,
                )) => {
                    // Auth-form round trip wins over the Secrets-tab
                    // dispatch: the auth form sets
                    // the modal parent stack exactly when it's the
                    // caller, so the two paths can never collide.
                    if editor.has_modal_parent() {
                        // Close the OpPicker — the auth form stays stashed on
                        // modal_parents so the _committed / _failed helpers find it.
                        editor.dismiss_active_modal();
                        return EditorModalOutcome::ValidateOpRef(op_ref);
                    }
                    // Operator picked a Vault → Item → Field path. The
                    // dispatch depends on whether `P` was pressed on a
                    // key row (write directly) or on an `+ Add` sentinel
                    // (stash the OpRef, ask for the key name first).
                    match secrets_target {
                        Some(SecretsPickerTarget::Existing { scope, key }) => {
                            super::set_pending_env_op_ref(editor, &scope, &key, op_ref);
                            editor.clear_modal_chain();
                        }
                        Some(SecretsPickerTarget::NewKey { scope }) => {
                            let label = secret_new_key_after_picker_label(&scope);
                            let state = env_key_input_state(editor, &scope, label, "");
                            editor.open_sub_modal(Modal::TextInput {
                                target: TextInputTarget::EnvKeyWithValue {
                                    scope: scope.clone(),
                                    value: jackin_core::EnvValue::OpRef(op_ref),
                                },
                                state,
                            });
                        }
                        None => {
                            editor.clear_modal_chain();
                        }
                    }
                }
                InlinePickerPlan::Dismiss => {
                    // Auth-form round trip: re-mount the form
                    // unchanged. Mirrors the Commit branch — the two
                    // callers (Secrets-tab `P`, auth-form Enter) are
                    // disambiguated by the modal parent stack.
                    if editor.has_modal_parent() {
                        super::super::auth::restore_auth_form_after_op_picker_cancel(editor);
                        return EditorModalOutcome::Continue;
                    }
                    editor.pop_modal_chain();
                }
                InlinePickerPlan::Continue => {}
            }
        }
    }
    EditorModalOutcome::Continue
}
