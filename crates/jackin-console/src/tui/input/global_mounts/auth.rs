#![expect(
    clippy::too_many_lines,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
//! Settings Auth tab key and modal handlers.

use super::{
    AuthForm, AuthFormFocus, AuthFormKeyPlan, AuthFormTarget, GlobalMountConfirm, KeyCode,
    KeyEvent, ManagerMessage, ManagerStage, ManagerState, SettingsAuthKeyPlan, SettingsAuthOutcome,
    SettingsModal, auth_credential_input_state, auth_form_key_plan_with_source_folder,
    auth_source_picker_state, confirm_modal, dispatch_manager, generated_token_op_item_name,
    generated_token_source_picker_state, open_settings_save_preview, settings_update,
};
use crate::tui::auth_config::settings_auth_form_can_generate_token;
use crate::tui::components::file_browser::page_rows_for_modal;
use crate::tui::update::{
    AuthSourceFolderPickerPlan, CreateOpPickerPlan, InlinePickerPlan, SourcePickerPlan,
    auth_source_folder_picker_plan, create_op_picker_plan, inline_picker_plan, source_picker_plan,
};

fn record_missing_auth_return_path() {
    let _recorded = jackin_telemetry::record_error(
        jackin_telemetry::schema::enums::ErrorType::TelemetryInstrumentationFault,
    );
}

pub(super) fn handle_auth_key(state: &mut ManagerState<'_>, key: KeyEvent) {
    let ManagerStage::Settings(settings) = &state.stage else {
        return;
    };
    let plan = settings_update::settings_auth_key_plan(
        key.code,
        settings.is_dirty(),
        settings.auth.has_selected_kind(),
        settings.auth.selected_detail_row_is_focusable(),
    );
    match plan {
        SettingsAuthKeyPlan::ClearKind => {
            dispatch_manager(state, ManagerMessage::ClearSettingsAuthKind);
        }
        SettingsAuthKeyPlan::MoveSelection { delta } => {
            dispatch_manager(state, ManagerMessage::MoveSettingsAuthSelection { delta });
        }
        SettingsAuthKeyPlan::EnterKind => {
            dispatch_manager(state, ManagerMessage::EnterSettingsAuthKind);
        }
        SettingsAuthKeyPlan::ConfirmDiscard => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                return;
            };
            if settings.is_dirty() {
                settings
                    .mounts
                    .modals
                    .open(confirm_modal(GlobalMountConfirm::Discard));
            }
        }
        SettingsAuthKeyPlan::ReturnToList => {
            dispatch_manager(state, ManagerMessage::ReturnToList);
        }
        SettingsAuthKeyPlan::OpenForm => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                return;
            };
            open_settings_auth_form(&mut settings.auth, &settings.env);
        }
        SettingsAuthKeyPlan::Save => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                return;
            };
            open_settings_save_preview(settings);
        }
        SettingsAuthKeyPlan::Noop => {}
    }
}

pub(crate) fn open_settings_auth_form(
    auth: &mut crate::tui::state::SettingsAuthState,
    env: &crate::tui::state::SettingsEnvState<'_>,
) {
    auth.open_selected_auth_modal(&env.pending.env, |kind, row, existing_credential| {
        let form = AuthForm::from_existing(kind, row.mode, existing_credential).with_source_folder(
            row.sync_source_dir.clone(),
            Some(crate::tui::auth_config::settings_source_folder_display(row)),
        );
        let literal_buffer = form.literal_buffer();
        SettingsModal::AuthForm {
            target: AuthFormTarget::Workspace { kind },
            state: Box::new(form),
            focus: AuthFormFocus::Mode,
            literal_buffer,
        }
    });
}

/// Whether the open settings Auth modal is eligible for the `g`/`G`
/// generate trigger: an `AuthForm` showing the global Claude
/// `oauth_token` slot. Settings generate is always global Claude, so —
/// unlike the workspace editor — there is no per-target gate.
pub fn settings_auth_can_generate_token(auth: &crate::tui::state::SettingsAuthState) -> bool {
    matches!(
        auth.modal_ref(),
        Some(SettingsModal::AuthForm { state, .. })
            if settings_auth_form_can_generate_token(state.kind, state.mode)
    )
}

/// Source-folder validation callback used by the settings auth modal.
type SourceFolderValidator =
    dyn Fn(Option<crate::tui::auth::AuthKind>, &std::path::Path) -> Result<(), String>;

#[expect(
    clippy::too_many_arguments,
    reason = "Settings-auth-modal key handler carries every per-binding input the \
              dispatch needs: auth state, env state, key event, palette key. \
              Same justification as the too_many_lines allow."
)]
pub fn handle_settings_auth_modal(
    auth: &mut crate::tui::state::SettingsAuthState,
    env: &mut crate::tui::state::SettingsEnvState<'_>,
    pending_token_generate: &mut Option<crate::tui::state::PendingTokenGenerate>,
    key: KeyEvent,
    op_available: bool,
    op_cache: std::rc::Rc<std::cell::RefCell<jackin_env::OpCache>>,
    term_size: ratatui::layout::Rect,
    validate_source_folder: &SourceFolderValidator,
) -> SettingsAuthOutcome {
    let Some(mut modal) = auth.take_modal() else {
        return SettingsAuthOutcome::Continue;
    };
    match &mut modal {
        SettingsModal::AuthForm {
            target,
            state,
            focus,
            literal_buffer: _,
        } => {
            if key.code == KeyCode::Esc {
                return SettingsAuthOutcome::Continue;
            }
            // `g`/`G` at any focus mints a global Claude OAuth token. It
            // opens the shared source picker (plain literal vs. 1Password)
            // first. Gated to the global Claude `oauth_token` slot; a
            // no-op for any other kind/mode. The open form is stashed so
            // the post-mint re-mount lands the operator back on the same
            // Edit-auth dialog with the minted credential staged, focus
            // Save — exactly like the provide path. Generate vs. provide
            // is disambiguated by the `generating_token` flag, which the
            // source-picker / op-picker commit arms check first.
            if matches!(key.code, KeyCode::Char('g' | 'G'))
                && settings_auth_form_can_generate_token(state.kind, state.mode)
            {
                auth.start_generating_token();
                // modal was taken from auth.modal at the start of this fn;
                // push it directly to preserve the in-progress form state.
                auth.open_child_modal(
                    modal,
                    SettingsModal::AuthSourcePicker {
                        state: generated_token_source_picker_state(op_available),
                    },
                );
                return SettingsAuthOutcome::Continue;
            }
            let plan = auth_form_key_plan_with_source_folder(
                *focus,
                key.code,
                state.shows_source_folder(),
                state.shows_credential_block(),
                state.can_save(),
            );
            match plan {
                AuthFormKeyPlan::Stay => {}
                AuthFormKeyPlan::Focus(next) => *focus = next,
                AuthFormKeyPlan::CycleMode => state.cycle_mode(),
                AuthFormKeyPlan::OpenCredentialSource => {
                    let Some(env_var) = state.mode.and_then(|m| state.kind.required_env_var(m))
                    else {
                        auth.set_modal(modal);
                        return SettingsAuthOutcome::Continue;
                    };
                    auth.open_child_modal(
                        modal,
                        SettingsModal::AuthSourcePicker {
                            state: auth_source_picker_state(env_var, op_available),
                        },
                    );
                    return SettingsAuthOutcome::Continue;
                }
                AuthFormKeyPlan::OpenSourceFolderBrowser => {
                    auth.set_modal(modal);
                    return SettingsAuthOutcome::OpenAuthSourceFolderBrowser;
                }
                AuthFormKeyPlan::Save => {
                    persist_settings_auth_form(auth, env, state);
                    return SettingsAuthOutcome::Continue;
                }
                AuthFormKeyPlan::Cancel => return SettingsAuthOutcome::Continue,
                AuthFormKeyPlan::Reset => {
                    clear_settings_auth_kind(auth, env, target);
                    return SettingsAuthOutcome::Continue;
                }
            }
            auth.set_modal(modal);
        }
        SettingsModal::AuthSourcePicker { state } => {
            let outcome = state.handle_key(key);
            // Generate wins over the provide dispatch: the `g`/`G` trigger
            // sets `generating_token` (and stashes the form into
            // the modal parent stack for the post-mint re-mount), so
            // the generate branch is reachable only on that path and the
            // provide arms below stay untouched.
            if auth.is_generating_token() {
                match source_picker_plan(outcome) {
                    SourcePickerPlan::Plain => {
                        auth.finish_generating_token();
                        *pending_token_generate = Some(crate::tui::state::PendingTokenGenerate {
                            scope: jackin_env::TokenSetupScope::Global,
                            args: jackin_env::TokenSetupArgs {
                                plain_text: true,
                                ..Default::default()
                            },
                        });
                    }
                    SourcePickerPlan::Op => {
                        // `generating_token` stays set so the Create-mode
                        // OpPicker commit routes through
                        // `handle_settings_token_generate_pick`.
                        auth.set_modal(SettingsModal::AuthOpPicker {
                            state: Box::new(
                                crate::tui::op_picker::OpPickerState::new_create_with_cache(
                                    op_cache,
                                    generated_token_op_item_name(
                                        jackin_env::DEFAULT_ITEM_TEMPLATE,
                                        "global",
                                    ),
                                    jackin_env::DEFAULT_FIELD_LABEL,
                                ),
                            ),
                        });
                    }
                    // Cancel before minting: restore the stashed form so
                    // the operator lands back on the Edit-auth dialog
                    // unchanged (matches the provide-path cancel below).
                    SourcePickerPlan::Dismiss => {
                        auth.finish_generating_token();
                        restore_settings_auth_form(auth);
                    }
                    SourcePickerPlan::Continue => auth.set_modal(modal),
                }
                return SettingsAuthOutcome::Continue;
            }
            match source_picker_plan(outcome) {
                SourcePickerPlan::Plain => {
                    let literal = auth
                        .modals
                        .parents()
                        .last()
                        .and_then(|m| {
                            if let SettingsModal::AuthForm { literal_buffer, .. } = m {
                                Some(literal_buffer.clone())
                            } else {
                                None
                            }
                        })
                        .unwrap_or_default();
                    auth.set_modal(SettingsModal::AuthTextInput {
                        state: Box::new(auth_credential_input_state(literal)),
                    });
                }
                SourcePickerPlan::Op => {
                    auth.set_modal(SettingsModal::AuthOpPicker {
                        state: Box::new(crate::tui::op_picker::OpPickerState::new_with_cache(
                            op_cache,
                        )),
                    });
                }
                SourcePickerPlan::Dismiss => restore_settings_auth_form(auth),
                SourcePickerPlan::Continue => auth.set_modal(modal),
            }
        }
        SettingsModal::AuthTextInput { state } => {
            match inline_picker_plan(state.handle_key(key.into())) {
                InlinePickerPlan::Commit(value) => {
                    apply_plain_text_to_settings_auth_form(auth, &value);
                }
                InlinePickerPlan::Dismiss => restore_settings_auth_form(auth),
                InlinePickerPlan::Continue => auth.set_modal(modal),
            }
        }
        SettingsModal::AuthSourceFolderPicker { state } => {
            let page_rows = page_rows_for_modal(term_size, state);
            let browser_outcome = state.handle_key_with_page_rows(key, Some(page_rows));
            match browser_outcome {
                crate::tui::components::file_browser::FileBrowserOutcome::NavigateTo(_)
                | crate::tui::components::file_browser::FileBrowserOutcome::NavigateUp
                | crate::tui::components::file_browser::FileBrowserOutcome::RequestCommit(_) => {
                    auth.set_modal(modal);
                    return SettingsAuthOutcome::ApplyFileBrowserOutcome(browser_outcome);
                }
                other => {
                    match auth_source_folder_picker_plan(other) {
                        AuthSourceFolderPickerPlan::Commit(path) => {
                            match validate_source_folder(auth.selected_kind(), &path) {
                                Ok(()) => apply_source_folder_to_settings_auth_form(auth, path),
                                // Wrong folder for this agent: keep the picker open and
                                // raise the standard error dialog (promoted from
                                // `auth.error`) over it, rather than committing a folder
                                // that yields no credentials. Dismissing the dialog
                                // leaves the picker so the operator can pick another.
                                Err(reason) => {
                                    auth.set_error(reason);
                                    auth.set_modal(modal);
                                }
                            }
                        }
                        AuthSourceFolderPickerPlan::Close => {}
                        AuthSourceFolderPickerPlan::KeepModal => {
                            auth.set_modal(modal);
                        }
                    }
                }
            }
        }
        SettingsModal::AuthOpPicker { state } => {
            let outcome = state.handle_key(key);
            // Token-generate wins over the browse/provide dispatch:
            // `generating_token` is set exactly when the picker was opened
            // by the auth-form `g`/`G` trigger (Create mode), so the create
            // variants are reachable only on this path.
            if auth.is_generating_token() {
                handle_settings_token_generate_pick(auth, pending_token_generate, outcome, modal);
                return SettingsAuthOutcome::Continue;
            }
            match crate::tui::update::op_picker_inline_plan(outcome) {
                // Browse-mode caller: only `Existing` is reachable.
                InlinePickerPlan::Commit(
                    crate::tui::op_picker::OpPickerSelection::NewItem { .. }
                    | crate::tui::op_picker::OpPickerSelection::EditItemField { .. },
                ) => unreachable!("settings-auth browse OpPicker runs in Browse mode"),
                InlinePickerPlan::Commit(crate::tui::op_picker::OpPickerSelection::Existing(
                    op_ref,
                )) => {
                    // Close the OpPicker — the auth form stays stashed on
                    // modal_parents so the _committed / _failed helpers find it.
                    auth.clear_modal();
                    return SettingsAuthOutcome::ValidateOpRef(op_ref);
                }
                InlinePickerPlan::Dismiss => restore_settings_auth_form(auth),
                InlinePickerPlan::Continue => auth.set_modal(modal),
            }
        }
        _ => unreachable!("auth input handler received a non-auth settings modal"),
    }
    SettingsAuthOutcome::Continue
}

/// Translate a Create-mode `OpPicker` commit into a global
/// [`PendingTokenGenerate`](crate::tui::state::PendingTokenGenerate)
/// request that the `run_console` loop drains to mint the token.
/// `Existing` cannot occur in Create mode; a Cancel (or stray
/// `Existing`) just closes the chain. On `Continue` the picker is still
/// drilling, so the marker stays armed and the modal stays open.
fn handle_settings_token_generate_pick(
    auth: &mut crate::tui::state::SettingsAuthState,
    pending_token_generate: &mut Option<crate::tui::state::PendingTokenGenerate>,
    outcome: jackin_oppicker::ModalOutcome<crate::tui::op_picker::OpPickerSelection>,
    modal: SettingsModal<'static>,
) {
    use crate::tui::op_picker::OpPickerSelection;
    use jackin_env::{EditExistingTarget, TokenSetupArgs};

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
        // Still drilling — leave the picker open and stay armed.
        CreateOpPickerPlan::Continue => {
            auth.set_modal(modal);
            return;
        }
        // `Existing` is unreachable in Create mode; a Cancel restores the
        // stashed form. Both close without minting and disarm the marker.
        CreateOpPickerPlan::Dismiss => {
            auth.finish_generating_token();
            restore_settings_auth_form(auth);
            return;
        }
    };

    auth.finish_generating_token();
    *pending_token_generate = Some(crate::tui::state::PendingTokenGenerate {
        scope: jackin_env::TokenSetupScope::Global,
        args,
    });
}

fn restore_settings_auth_form(auth: &mut crate::tui::state::SettingsAuthState) {
    auth.restore_pending_auth_form();
}

/// Lift the stashed settings auth form, apply a literal credential, and
/// re-mount it with focus on Save. Shared by the provide-path
/// `TextInput` commit and the post-mint plain-text generate re-mount in
/// the `run_console` loop — both stage a literal and drop the operator
/// onto Save so the editor's normal save persists it.
pub fn apply_plain_text_to_settings_auth_form(
    auth: &mut crate::tui::state::SettingsAuthState,
    value: &str,
) {
    let Some(SettingsModal::AuthForm {
        target, mut state, ..
    }) = auth.pop_parent_modal()
    else {
        record_missing_auth_return_path();
        return;
    };
    state.set_literal(value.to_owned());
    auth.set_modal(SettingsModal::AuthForm {
        target,
        state,
        focus: AuthFormFocus::Save,
        literal_buffer: value.to_owned(),
    });
}

pub(crate) fn apply_source_folder_to_settings_auth_form(
    auth: &mut crate::tui::state::SettingsAuthState,
    path: std::path::PathBuf,
) {
    let Some(SettingsModal::AuthForm {
        target,
        mut state,
        literal_buffer,
        ..
    }) = auth.pop_parent_modal()
    else {
        record_missing_auth_return_path();
        return;
    };
    state.set_source_folder(path);
    auth.set_modal(SettingsModal::AuthForm {
        target,
        state,
        focus: AuthFormFocus::Save,
        literal_buffer,
    });
}

/// Lift the stashed settings auth form, read-back-validate a picked
/// `OpRef` against the account it carries, and re-mount the form with
/// focus on Save. On a read failure the form is re-stashed and the
/// error surfaced through the auth error slot so the operator can retry. Shared
/// by the provide-path `OpPicker` commit and the post-mint op generate
/// re-mount in the `run_console` loop.
/// Inner helper split out so tests can inject a fake `OpRunner` without
/// touching the real `op` binary (mirrors
/// `auth::apply_op_picker_to_auth_form_with_runner`).
#[cfg(test)]
pub(crate) fn apply_op_picker_to_settings_auth_form_with_runner<
    R: jackin_env::OpRunner + ?Sized,
>(
    auth: &mut crate::tui::state::SettingsAuthState,
    op_ref: jackin_core::OpRef,
    runner: &R,
) {
    apply_op_picker_to_settings_auth_form_with_validator(auth, op_ref, |op_ref| {
        runner.read(&op_ref.op).map(|_| ())
    });
}

#[cfg(test)]
fn apply_op_picker_to_settings_auth_form_with_validator(
    auth: &mut crate::tui::state::SettingsAuthState,
    op_ref: jackin_core::OpRef,
    validate: impl FnOnce(&jackin_core::OpRef) -> anyhow::Result<()>,
) {
    let Some(SettingsModal::AuthForm {
        target,
        mut state,
        focus,
        literal_buffer,
    }) = auth.pop_parent_modal()
    else {
        // Mirrors the editor twin's missing-stash breadcrumb: a minted
        // global token with no form to return to would otherwise vanish
        // silently. Should be unreachable (the `g`/`G` trigger always
        // stashes), so a hit here means a broken stash invariant.
        record_missing_auth_return_path();
        return;
    };
    match validate(&op_ref) {
        Ok(()) => {
            state.set_op_ref(op_ref);
            auth.set_modal(SettingsModal::AuthForm {
                target,
                state,
                focus: AuthFormFocus::Save,
                literal_buffer,
            });
        }
        Err(err) => {
            // The form is only mutated after a successful read; re-stash so a
            // later restore lands the operator back on the prior value.
            auth.push_auth_modal(SettingsModal::AuthForm {
                target,
                state,
                focus,
                literal_buffer,
            });
            auth.set_error(
                crate::tui::screens::settings::view::settings_auth_op_read_failed_message(err),
            );
        }
    }
}

/// Apply a committed op picker selection to the settings auth form after the
/// 1Password read has already succeeded on the `spawn_blocking` thread. Called
/// from the `run_console` poll loop — the read was verified asynchronously so
/// Touch ID / the 1Password desktop dialog did not freeze the TUI reactor.
///
/// The auth form is on `auth.modal_parents` — pop it, set the `OpRef` without
/// re-reading, and re-mount with focus on Save.
pub fn apply_op_picker_to_settings_auth_form_committed(
    auth: &mut crate::tui::state::SettingsAuthState,
    op_ref: jackin_core::OpRef,
) {
    let Some(SettingsModal::AuthForm {
        target,
        mut state,
        literal_buffer,
        ..
    }) = auth.pop_parent_modal()
    else {
        record_missing_auth_return_path();
        return;
    };
    // The read already succeeded; set the ref directly without re-reading.
    state.set_op_ref(op_ref);
    auth.set_modal(SettingsModal::AuthForm {
        target,
        state,
        focus: AuthFormFocus::Save,
        literal_buffer,
    });
}

fn persist_settings_auth_form(
    auth: &mut crate::tui::state::SettingsAuthState,
    env: &mut crate::tui::state::SettingsEnvState<'_>,
    form: &AuthForm,
) {
    let Some(outcome) = form.commit() else {
        return;
    };
    auth.apply_auth_outcome(form.kind, outcome, &mut env.pending.env);
}

fn clear_settings_auth_kind(
    auth: &mut crate::tui::state::SettingsAuthState,
    env: &mut crate::tui::state::SettingsEnvState<'_>,
    target: &AuthFormTarget,
) {
    let AuthFormTarget::Workspace { kind } = target else {
        return;
    };
    auth.clear_auth_kind(*kind, &mut env.pending.env);
}

#[cfg(test)]
mod tests;
