//! Settings Auth tab key and modal handlers.

use super::{
    AuthForm, AuthFormFocus, AuthFormKeyPlan, AuthFormTarget, FileBrowserOutcome,
    GlobalMountConfirm, KeyCode, KeyEvent, ManagerMessage, ManagerStage, ManagerState,
    ModalOutcome, SettingsAuthModal, SettingsAuthOutcome, SettingsStateExt,
    apply_settings_auth_env_commit, auth_credential_input_state,
    auth_form_key_plan_with_source_folder, auth_source_picker_state,
    can_generate_claude_oauth_token, clear_settings_auth_env_values, confirm_modal,
    dispatch_manager, generated_token_op_item_name, generated_token_source_picker_state,
    open_settings_save_preview, settings_auth_op_read_failed_message,
};

pub(super) fn handle_auth_key(state: &mut ManagerState<'_>, key: KeyEvent) {
    let ManagerStage::Settings(settings) = &state.stage else {
        return;
    };
    match key.code {
        KeyCode::Esc | KeyCode::Char('q' | 'Q') if settings.auth.selected_kind.is_some() => {
            dispatch_manager(state, ManagerMessage::ClearSettingsAuthKind);
            return;
        }
        KeyCode::Up | KeyCode::Char('k' | 'K') => {
            dispatch_manager(
                state,
                ManagerMessage::MoveSettingsAuthSelection { delta: -1 },
            );
            return;
        }
        KeyCode::Down | KeyCode::Char('j' | 'J') => {
            dispatch_manager(
                state,
                ManagerMessage::MoveSettingsAuthSelection { delta: 1 },
            );
            return;
        }
        KeyCode::Enter if settings.auth.selected_kind.is_none() => {
            dispatch_manager(state, ManagerMessage::EnterSettingsAuthKind);
            return;
        }
        _ => {}
    }

    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    let mut return_to_list = false;
    match key.code {
        KeyCode::Esc | KeyCode::Char('q' | 'Q') => {
            if settings.is_dirty() {
                settings.mounts.modal = Some(confirm_modal(GlobalMountConfirm::Discard));
            } else {
                return_to_list = true;
            }
        }
        KeyCode::Enter => {
            if selected_settings_auth_row_is_focusable(&settings.auth) {
                open_settings_auth_form(&mut settings.auth, &settings.env);
            }
        }
        KeyCode::Char('d' | 'D') => {}
        KeyCode::Char('s' | 'S') => {
            open_settings_save_preview(settings);
        }
        _ => {}
    }
    if return_to_list {
        dispatch_manager(state, ManagerMessage::ReturnToList);
    }
}

fn selected_settings_auth_detail_row(
    auth: &crate::console::tui::state::SettingsAuthState,
) -> Option<jackin_console::tui::screens::settings::update::SettingsAuthDetailRow> {
    let kind = auth.selected_kind?;
    let row = auth.pending.iter().find(|row| row.kind == kind)?;
    jackin_console::tui::screens::settings::update::settings_auth_detail_rows(kind, row.mode)
        .get(auth.selected)
        .copied()
}

fn selected_settings_auth_row_is_focusable(
    auth: &crate::console::tui::state::SettingsAuthState,
) -> bool {
    selected_settings_auth_detail_row(auth)
        .is_some_and(jackin_console::tui::screens::settings::update::settings_auth_row_is_focusable)
}

pub(super) fn open_settings_auth_form(
    auth: &mut crate::console::tui::state::SettingsAuthState,
    env: &crate::console::tui::state::SettingsEnvState<'_>,
) {
    let Some(kind) = auth.selected_kind else {
        return;
    };
    let Some(row) = auth.pending.iter().find(|row| row.kind == kind) else {
        return;
    };
    let existing_credential = crate::console::domain::settings_auth_env_value(
        kind,
        row.mode,
        &auth.github_env,
        &env.pending.env,
    )
    .cloned();
    let form = AuthForm::from_existing(kind, row.mode, existing_credential).with_source_folder(
        row.sync_source_dir.clone(),
        Some(crate::console::tui::components::auth_panel::settings_source_folder_display(row)),
    );
    let literal_buffer = form.literal_buffer();
    auth.modal = Some(SettingsAuthModal::AuthForm {
        target: AuthFormTarget::Workspace { kind },
        state: Box::new(form),
        focus: AuthFormFocus::Mode,
        literal_buffer,
    });
}

/// Whether the open settings Auth modal is eligible for the `g`/`G`
/// generate trigger: an `AuthForm` showing the global Claude
/// `oauth_token` slot. Settings generate is always global Claude, so —
/// unlike the workspace editor — there is no per-target gate.
pub(crate) fn settings_auth_can_generate_token(
    auth: &crate::console::tui::state::SettingsAuthState,
) -> bool {
    matches!(
        auth.modal.as_ref(),
        Some(SettingsAuthModal::AuthForm { state, .. })
            if can_generate_claude_oauth_token(state.kind, state.mode)
    )
}

#[expect(
    clippy::too_many_lines,
    reason = "pending extraction — tracked in codebase-readability roadmap"
)]
pub(in crate::console::tui::input) fn handle_settings_auth_modal(
    auth: &mut crate::console::tui::state::SettingsAuthState,
    env: &mut crate::console::tui::state::SettingsEnvState<'_>,
    pending_token_generate: &mut Option<crate::console::tui::state::PendingTokenGenerate>,
    key: KeyEvent,
    op_available: bool,
    op_cache: std::rc::Rc<std::cell::RefCell<crate::operator_env::OpCache>>,
    term_size: ratatui::layout::Rect,
) -> SettingsAuthOutcome {
    let Some(mut modal) = auth.modal.take() else {
        return SettingsAuthOutcome::Continue;
    };
    match &mut modal {
        SettingsAuthModal::AuthForm {
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
                && can_generate_claude_oauth_token(state.kind, state.mode)
            {
                auth.generating_token = true;
                // modal was taken from auth.modal at the start of this fn;
                // push it directly to preserve the in-progress form state.
                auth.modal_parents.push(modal);
                auth.modal = Some(SettingsAuthModal::SourcePicker {
                    state: generated_token_source_picker_state(op_available),
                });
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
                        auth.modal = Some(modal);
                        return SettingsAuthOutcome::Continue;
                    };
                    auth.modal_parents.push(modal);
                    auth.modal = Some(SettingsAuthModal::SourcePicker {
                        state: auth_source_picker_state(env_var, op_available),
                    });
                    return SettingsAuthOutcome::Continue;
                }
                AuthFormKeyPlan::OpenSourceFolderBrowser => {
                    match crate::console::services::file_browser::from_home_with_hidden() {
                        Ok(state) => {
                            auth.modal_parents.push(modal);
                            auth.modal = Some(SettingsAuthModal::SourceFolderPicker { state });
                        }
                        Err(error) => {
                            auth.error = Some(error.to_string());
                        }
                    }
                    return SettingsAuthOutcome::Continue;
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
            auth.modal = Some(modal);
        }
        SettingsAuthModal::SourcePicker { state } => {
            use jackin_console::tui::components::source_picker::SourceChoice;
            let outcome = state.handle_key(key);
            // Generate wins over the provide dispatch: the `g`/`G` trigger
            // sets `generating_token` (and stashes the form into
            // `pending_auth_form_return` for the post-mint re-mount), so
            // the generate branch is reachable only on that path and the
            // provide arms below stay untouched.
            if auth.generating_token {
                match outcome {
                    ModalOutcome::Commit(SourceChoice::Plain) => {
                        auth.generating_token = false;
                        *pending_token_generate =
                            Some(crate::console::tui::state::PendingTokenGenerate {
                                scope: crate::workspace::token_setup::TokenSetupScope::Global,
                                args: crate::workspace::token_setup::TokenSetupArgs {
                                    plain_text: true,
                                    ..Default::default()
                                },
                            });
                    }
                    ModalOutcome::Commit(SourceChoice::Op) => {
                        // `generating_token` stays set so the Create-mode
                        // OpPicker commit routes through
                        // `handle_settings_token_generate_pick`.
                        auth.modal = Some(SettingsAuthModal::OpPicker {
                            state: Box::new(
                                crate::console::tui::op_picker::OpPickerState::new_create_with_cache(
                                    op_cache,
                                    generated_token_op_item_name(
                                        crate::workspace::token_setup::DEFAULT_ITEM_TEMPLATE,
                                        "global",
                                    ),
                                    crate::workspace::token_setup::DEFAULT_FIELD_LABEL,
                                ),
                            ),
                        });
                    }
                    // Cancel before minting: restore the stashed form so
                    // the operator lands back on the Edit-auth dialog
                    // unchanged (matches the provide-path cancel below).
                    ModalOutcome::Cancel => {
                        auth.generating_token = false;
                        restore_settings_auth_form(auth);
                    }
                    ModalOutcome::Continue => auth.modal = Some(modal),
                }
                return SettingsAuthOutcome::Continue;
            }
            match outcome {
                ModalOutcome::Commit(SourceChoice::Plain) => {
                    let literal = auth
                        .modal_parents
                        .last()
                        .and_then(|m| {
                            if let SettingsAuthModal::AuthForm { literal_buffer, .. } = m {
                                Some(literal_buffer.clone())
                            } else {
                                None
                            }
                        })
                        .unwrap_or_default();
                    auth.modal = Some(SettingsAuthModal::TextInput {
                        state: Box::new(auth_credential_input_state(literal)),
                    });
                }
                ModalOutcome::Commit(SourceChoice::Op) => {
                    auth.modal = Some(SettingsAuthModal::OpPicker {
                        state: Box::new(
                            crate::console::tui::op_picker::OpPickerState::new_with_cache(op_cache),
                        ),
                    });
                }
                ModalOutcome::Cancel => restore_settings_auth_form(auth),
                ModalOutcome::Continue => auth.modal = Some(modal),
            }
        }
        SettingsAuthModal::TextInput { state } => match state.handle_key(key) {
            ModalOutcome::Commit(value) => apply_plain_text_to_settings_auth_form(auth, &value),
            ModalOutcome::Cancel => restore_settings_auth_form(auth),
            ModalOutcome::Continue => auth.modal = Some(modal),
        },
        SettingsAuthModal::SourceFolderPicker { state } => {
            let page_rows = super::file_browser_page_rows(term_size, state);
            let browser_outcome = state.handle_key_with_page_rows(key, Some(page_rows));
            let applied = crate::console::services::file_browser::apply_file_browser_outcome(
                state,
                browser_outcome,
            );
            match applied {
                FileBrowserOutcome::Commit(path) => {
                    match validate_picked_source_folder(auth, &path) {
                        Ok(()) => apply_source_folder_to_settings_auth_form(auth, path),
                        // Wrong folder for this agent: keep the picker open and
                        // raise the standard error dialog (promoted from
                        // `auth.error`) over it, rather than committing a folder
                        // that yields no credentials. Dismissing the dialog
                        // leaves the picker so the operator can pick another.
                        Err(reason) => {
                            auth.error = Some(reason);
                            auth.modal = Some(modal);
                        }
                    }
                }
                FileBrowserOutcome::Cancel => {}
                FileBrowserOutcome::Continue
                | FileBrowserOutcome::OpenGitUrl(_)
                | FileBrowserOutcome::ResolveGitUrl(_)
                | FileBrowserOutcome::NavigateTo(_)
                | FileBrowserOutcome::NavigateUp
                | FileBrowserOutcome::RequestCommit(_) => {
                    auth.modal = Some(modal);
                }
            }
        }
        SettingsAuthModal::OpPicker { state } => {
            let outcome = state.handle_key(key);
            // Token-generate wins over the browse/provide dispatch:
            // `generating_token` is set exactly when the picker was opened
            // by the auth-form `g`/`G` trigger (Create mode), so the create
            // variants are reachable only on this path.
            if auth.generating_token {
                handle_settings_token_generate_pick(auth, pending_token_generate, outcome, modal);
                return SettingsAuthOutcome::Continue;
            }
            match outcome {
                // Browse-mode caller: only `Existing` is reachable.
                ModalOutcome::Commit(
                    crate::console::tui::op_picker::OpPickerSelection::NewItem { .. }
                    | crate::console::tui::op_picker::OpPickerSelection::EditItemField { .. },
                ) => unreachable!("settings-auth browse OpPicker runs in Browse mode"),
                ModalOutcome::Commit(
                    crate::console::tui::op_picker::OpPickerSelection::Existing(op_ref),
                ) => {
                    // Close the OpPicker — the auth form stays stashed on
                    // modal_parents so the _committed / _failed helpers find it.
                    auth.modal = None;
                    return SettingsAuthOutcome::ValidateOpRef(op_ref);
                }
                ModalOutcome::Cancel => restore_settings_auth_form(auth),
                ModalOutcome::Continue => auth.modal = Some(modal),
            }
        }
    }
    SettingsAuthOutcome::Continue
}

/// Translate a Create-mode `OpPicker` commit into a global
/// [`PendingTokenGenerate`](crate::console::tui::state::PendingTokenGenerate)
/// request that the `run_console` loop drains to mint the token.
/// `Existing` cannot occur in Create mode; a Cancel (or stray
/// `Existing`) just closes the chain. On `Continue` the picker is still
/// drilling, so the marker stays armed and the modal stays open.
fn handle_settings_token_generate_pick(
    auth: &mut crate::console::tui::state::SettingsAuthState,
    pending_token_generate: &mut Option<crate::console::tui::state::PendingTokenGenerate>,
    outcome: ModalOutcome<crate::console::tui::op_picker::OpPickerSelection>,
    modal: SettingsAuthModal<'static>,
) {
    use crate::console::tui::op_picker::OpPickerSelection;
    use crate::workspace::token_setup::{EditExistingTarget, TokenSetupArgs};

    let args = match outcome {
        ModalOutcome::Commit(OpPickerSelection::NewItem {
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
        ModalOutcome::Commit(OpPickerSelection::EditItemField {
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
        // Still drilling — leave the picker open and stay armed.
        ModalOutcome::Continue => {
            auth.modal = Some(modal);
            return;
        }
        // `Existing` is unreachable in Create mode; a Cancel restores the
        // stashed form. Both close without minting and disarm the marker.
        ModalOutcome::Commit(OpPickerSelection::Existing(_)) | ModalOutcome::Cancel => {
            auth.generating_token = false;
            restore_settings_auth_form(auth);
            return;
        }
    };

    auth.generating_token = false;
    *pending_token_generate = Some(crate::console::tui::state::PendingTokenGenerate {
        scope: crate::workspace::token_setup::TokenSetupScope::Global,
        args,
    });
}

fn restore_settings_auth_form(auth: &mut crate::console::tui::state::SettingsAuthState) {
    auth.restore_pending_auth_form();
}

/// Lift the stashed settings auth form, apply a literal credential, and
/// re-mount it with focus on Save. Shared by the provide-path
/// `TextInput` commit and the post-mint plain-text generate re-mount in
/// the `run_console` loop — both stage a literal and drop the operator
/// onto Save so the editor's normal save persists it.
pub(in crate::console) fn apply_plain_text_to_settings_auth_form(
    auth: &mut crate::console::tui::state::SettingsAuthState,
    value: &str,
) {
    let Some(SettingsAuthModal::AuthForm {
        target, mut state, ..
    }) = auth.modal_parents.pop()
    else {
        crate::debug_log!(
            "auth",
            "apply_plain_text_to_settings_auth_form: pending_auth_form_return missing — \
             minted plain token dropped"
        );
        return;
    };
    state.set_literal(value.to_owned());
    auth.modal = Some(SettingsAuthModal::AuthForm {
        target,
        state,
        focus: AuthFormFocus::Save,
        literal_buffer: value.to_owned(),
    });
}

/// Validate a picked source folder against the agent the settings auth
/// form targets. Delegates to the shared console-domain validator so the
/// settings and workspace-editor pickers behave identically.
fn validate_picked_source_folder(
    auth: &crate::console::tui::state::SettingsAuthState,
    path: &std::path::Path,
) -> Result<(), String> {
    crate::console::domain::validate_auth_source_folder(auth.selected_kind, path)
}

fn apply_source_folder_to_settings_auth_form(
    auth: &mut crate::console::tui::state::SettingsAuthState,
    path: std::path::PathBuf,
) {
    let Some(SettingsAuthModal::AuthForm {
        target,
        mut state,
        literal_buffer,
        ..
    }) = auth.modal_parents.pop()
    else {
        crate::debug_log!(
            "auth",
            "apply_source_folder_to_settings_auth_form: modal_parents missing — path dropped"
        );
        return;
    };
    state.set_source_folder(path);
    auth.modal = Some(SettingsAuthModal::AuthForm {
        target,
        state,
        focus: AuthFormFocus::Save,
        literal_buffer,
    });
}

/// Lift the stashed settings auth form, read-back-validate a picked
/// `OpRef` against the account it carries, and re-mount the form with
/// focus on Save. On a read failure the form is re-stashed and the
/// error surfaced through `auth.error` so the operator can retry. Shared
/// by the provide-path `OpPicker` commit and the post-mint op generate
/// re-mount in the `run_console` loop.
/// Inner helper split out so tests can inject a fake `OpRunner` without
/// touching the real `op` binary (mirrors
/// `auth::apply_op_picker_to_auth_form_with_runner`).
#[cfg(test)]
pub(super) fn apply_op_picker_to_settings_auth_form_with_runner<
    R: crate::operator_env::OpRunner + ?Sized,
>(
    auth: &mut crate::console::tui::state::SettingsAuthState,
    op_ref: crate::operator_env::OpRef,
    runner: &R,
) {
    apply_op_picker_to_settings_auth_form_with_validator(auth, op_ref, |op_ref| {
        runner.read(&op_ref.op).map(|_| ())
    });
}

#[cfg(test)]
fn apply_op_picker_to_settings_auth_form_with_validator(
    auth: &mut crate::console::tui::state::SettingsAuthState,
    op_ref: crate::operator_env::OpRef,
    validate: impl FnOnce(&crate::operator_env::OpRef) -> anyhow::Result<()>,
) {
    let Some(SettingsAuthModal::AuthForm {
        target,
        mut state,
        focus,
        literal_buffer,
    }) = auth.modal_parents.pop()
    else {
        // Mirrors the editor twin's missing-stash breadcrumb: a minted
        // global token with no form to return to would otherwise vanish
        // silently. Should be unreachable (the `g`/`G` trigger always
        // stashes), so a hit here means a broken stash invariant.
        crate::debug_log!(
            "auth",
            "apply_op_picker_to_settings_auth_form: pending_auth_form_return missing — \
             minted op ref dropped"
        );
        return;
    };
    match validate(&op_ref) {
        Ok(()) => {
            state.set_op_ref(op_ref);
            auth.modal = Some(SettingsAuthModal::AuthForm {
                target,
                state,
                focus: AuthFormFocus::Save,
                literal_buffer,
            });
        }
        Err(err) => {
            // The form is only mutated after a successful read; re-stash so a
            // later restore lands the operator back on the prior value.
            auth.push_auth_modal(SettingsAuthModal::AuthForm {
                target,
                state,
                focus,
                literal_buffer,
            });
            auth.error = Some(settings_auth_op_read_failed_message(err));
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
pub(in crate::console) fn apply_op_picker_to_settings_auth_form_committed(
    auth: &mut crate::console::tui::state::SettingsAuthState,
    op_ref: crate::operator_env::OpRef,
) {
    let Some(SettingsAuthModal::AuthForm {
        target,
        mut state,
        literal_buffer,
        ..
    }) = auth.modal_parents.pop()
    else {
        crate::debug_log!(
            "auth",
            "apply_op_picker_to_settings_auth_form_committed: modal_parents missing \
             — async OpRef commit dropped"
        );
        return;
    };
    // The read already succeeded; set the ref directly without re-reading.
    state.set_op_ref(op_ref);
    auth.modal = Some(SettingsAuthModal::AuthForm {
        target,
        state,
        focus: AuthFormFocus::Save,
        literal_buffer,
    });
}

/// Called when the async 1Password read for a settings auth-form op picker
/// commit fails (Touch ID rejected, network error, vault not found, etc.).
/// Surfaces the error through `auth.error` (same slot the synchronous path
/// used); the auth form stays stashed on `auth.modal_parents` so
/// `restore_settings_auth_form` can bring it back on the next user action.
pub(in crate::console) fn apply_op_picker_settings_commit_failed(
    auth: &mut crate::console::tui::state::SettingsAuthState,
    error: &anyhow::Error,
) {
    auth.error = Some(settings_auth_op_read_failed_message(error));
}

fn persist_settings_auth_form(
    auth: &mut crate::console::tui::state::SettingsAuthState,
    env: &mut crate::console::tui::state::SettingsEnvState<'_>,
    form: &AuthForm,
) {
    let Some(outcome) = form.commit() else {
        return;
    };
    if let Some(row) = auth.pending.iter_mut().find(|row| row.kind == form.kind) {
        row.mode = outcome.mode;
        row.sync_source_dir = outcome.source_folder;
    }
    apply_settings_auth_env_commit(
        form.kind,
        outcome.env_var_name,
        outcome.env_value,
        &mut auth.github_env,
        &mut env.pending.env,
    );
    auth.selected = auth.selected.min(auth.row_count().saturating_sub(1));
}

fn clear_settings_auth_kind(
    auth: &mut crate::console::tui::state::SettingsAuthState,
    env: &mut crate::console::tui::state::SettingsEnvState<'_>,
    target: &AuthFormTarget,
) {
    let AuthFormTarget::Workspace { kind } = target else {
        return;
    };
    if let Some(row) = auth.pending.iter_mut().find(|row| row.kind == *kind) {
        row.mode = jackin_console::tui::auth::AuthMode::Sync;
        row.sync_source_dir = None;
    }
    clear_settings_auth_env_values(*kind, &mut auth.github_env, &mut env.pending.env);
}
