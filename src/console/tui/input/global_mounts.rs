use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::console::domain::{
    apply_settings_auth_env_commit, clear_settings_auth_env_values,
};
use crate::console::tui::message::{ManagerMessage, update_manager};
use crate::console::tui::components::mount_display::settings_global_mounts_content_width_with_cache;
use crate::console::tui::state::{
    AuthFormFocus, AuthFormTarget, GlobalMountConfirm, GlobalMountDraft, GlobalMountModal,
    GlobalMountTextTarget, ManagerStage, ManagerState, SettingsAuthModal, SettingsEnvConfirm,
    SettingsEnvEnterPlan, SettingsEnvModal, SettingsEnvRow, SettingsEnvScope,
    SettingsEnvTextTarget, SettingsTab, settings_env_flat_rows, settings_env_state_flat_rows,
};
use crate::console::tui::effect::ManagerEffect;
use jackin_tui::ModalOutcome;
use crate::console::tui::components::auth_panel::{AuthForm, CredentialInput};
use crate::selector::RolePickerState;
use crate::selector::RoleSelector;
use crate::workspace::resolve_path;
use jackin_console::tui::components::auth_panel::{
    AuthFormKeyPlan, auth_credential_input_state, auth_form_key_plan, auth_source_picker_state,
    generated_token_op_item_name, generated_token_source_picker_state,
};
use jackin_console::tui::components::file_browser::FileBrowserOutcome;
use jackin_console::tui::auth::can_generate_claude_oauth_token;
use jackin_console::tui::screens::settings::update as settings_update;
use jackin_console::tui::screens::settings::view::{
    global_mount_add_cancelled_message, global_mount_confirm_state,
    global_mount_add_draft_lost_message, global_mount_destination_empty_message,
    global_mount_gone_message, global_mount_name_empty_message,
    global_mount_no_github_url_message,
    global_mount_scope_picker_state, global_mount_scope_text_value,
    global_mount_text_input_state, global_mount_text_target_label,
    settings_auth_op_read_failed_message, settings_env_delete_confirm_state,
    settings_env_add_cancelled_message, settings_env_edit_cancelled_message,
    settings_env_empty_key_error_message,
    settings_env_empty_key_label, settings_env_key_input_state,
    settings_env_new_key_after_picker_label, settings_env_new_key_label,
    settings_error_popup_title, settings_sensitive_paths_not_confirmed_message,
    settings_env_scope_picker_state, settings_env_source_picker_state, settings_env_text_input_state,
    settings_env_value_current_text, settings_env_value_text_label,
    settings_no_registered_roles_error_message,
};

pub(super) type SettingsModalOutcome =
    jackin_console::tui::message::ConsoleSettingsModalOutcome;

pub(super) type SettingsAuthOutcome =
    jackin_console::tui::message::ConsoleSettingsAuthOutcome<crate::operator_env::OpRef>;

#[cfg(test)]
pub(super) fn handle_settings_key(state: &mut ManagerState<'_>, key: KeyEvent) {
    handle_settings_key_with_effects(state, key);
}

pub(super) fn handle_settings_key_with_effects(state: &mut ManagerState<'_>, key: KeyEvent) {
    let ManagerStage::Settings(settings) = &state.stage else {
        return;
    };

    // W3C ARIA Tabs: when tab_bar_focused, Left/Right cycle tabs and Tab/↓
    // enters the content area.
    if settings.tab_bar_focused {
        match key.code {
            KeyCode::Left | KeyCode::BackTab => {
                dispatch_manager(
                    state,
                    ManagerMessage::MoveSettingsTab {
                        delta: -1,
                        focus_tab_bar: true,
                    },
                );
                return;
            }
            KeyCode::Right => {
                dispatch_manager(
                    state,
                    ManagerMessage::MoveSettingsTab {
                        delta: 1,
                        focus_tab_bar: true,
                    },
                );
                return;
            }
            KeyCode::Tab | KeyCode::Down | KeyCode::Char('j') => {
                dispatch_manager(state, ManagerMessage::FocusSettingsContent);
                return;
            }
            _ => {}
        }
        // All other keys (S, Esc, etc.) fall through to content handling.
    }

    match key.code {
        KeyCode::Tab => {
            dispatch_manager(
                state,
                ManagerMessage::MoveSettingsTab {
                    delta: 1,
                    focus_tab_bar: true,
                },
            );
            return;
        }
        KeyCode::BackTab => {
            dispatch_manager(state, ManagerMessage::FocusSettingsTabBar);
            return;
        }
        KeyCode::Esc if !settings.tab_bar_focused => {
            if settings.auth.selected_kind.is_some() {
                dispatch_manager(state, ManagerMessage::ClearSettingsAuthKind);
            }
            dispatch_manager(state, ManagerMessage::FocusSettingsTabBar);
            return;
        }
        _ => {}
    }

    let ManagerStage::Settings(settings) = &state.stage else {
        return;
    };
    match key.code {
        // Right on an Environments role header expands it; Right elsewhere is
        // intra-area and must not cycle tabs.
        KeyCode::Right if settings.active_tab == SettingsTab::Environments => {
            let rows = settings_env_flat_rows(settings);
            if let Some(SettingsEnvRow::RoleHeader { role, expanded }) =
                rows.get(settings.env.selected).cloned()
                && !expanded
            {
                dispatch_manager(
                    state,
                    ManagerMessage::SetSettingsEnvRoleExpanded {
                        role,
                        expanded: true,
                    },
                );
            }
            return;
        }
        // Left on an Environments role header collapses it.
        KeyCode::Left if settings.active_tab == SettingsTab::Environments => {
            let rows = settings_env_flat_rows(settings);
            if let Some(SettingsEnvRow::RoleHeader { role, expanded }) =
                rows.get(settings.env.selected).cloned()
                && expanded
            {
                dispatch_manager(
                    state,
                    ManagerMessage::SetSettingsEnvRoleExpanded {
                        role,
                        expanded: false,
                    },
                );
            }
            return;
        }
        _ => {}
    }
    match settings.active_tab {
        SettingsTab::General => handle_general_key(state, key),
        SettingsTab::Mounts => handle_global_mounts_key(state, key),
        SettingsTab::Environments => handle_env_key(state, key),
        SettingsTab::Auth => handle_auth_key(state, key),
        SettingsTab::Trust => handle_trust_key(state, key),
    }
}

fn dispatch_manager(state: &mut ManagerState<'_>, message: ManagerMessage) {
    let _dirty = update_manager(state, message);
}

#[allow(clippy::too_many_lines)]
fn handle_global_mounts_key(
    state: &mut ManagerState<'_>,
    key: KeyEvent,
) {
    // S is handled here, before `global` borrows `settings.mounts`, so
    // `open_settings_save_preview` can receive all of `settings`.
    if matches!(key.code, KeyCode::Char('s' | 'S')) {
        let ManagerStage::Settings(settings) = &mut state.stage else {
            return;
        };
        if crate::console::domain::global_rows_have_sensitive_mount(&settings.mounts.pending) {
            settings.mounts.modal = Some(confirm_modal(GlobalMountConfirm::Sensitive));
        } else {
            open_settings_save_preview(settings);
        }
        return;
    }

    let ManagerStage::Settings(settings) = &state.stage else {
        return;
    };
    let term_width = state.cached_term_size.width;
    let content_width = settings_global_mounts_content_width_with_cache(
        &settings.mounts.pending,
        &settings.mounts.mount_info_cache,
    );
    let footer_h = settings.cached_footer_h;
    match key.code {
        KeyCode::Char('h' | 'H') => {
            dispatch_manager(
                state,
                ManagerMessage::ScrollSettingsGlobalMountsHorizontal {
                    delta: -8,
                    term_width,
                    content_width,
                },
            );
            return;
        }
        KeyCode::Char('l' | 'L') => {
            dispatch_manager(
                state,
                ManagerMessage::ScrollSettingsGlobalMountsHorizontal {
                    delta: 8,
                    term_width,
                    content_width,
                },
            );
            return;
        }
        KeyCode::Up | KeyCode::Char('k' | 'K') => {
            dispatch_manager(
                state,
                ManagerMessage::MoveSettingsGlobalMountsSelection {
                    delta: -1,
                    term: state.cached_term_size,
                    footer_h,
                },
            );
            return;
        }
        KeyCode::Down | KeyCode::Char('j' | 'J') => {
            dispatch_manager(
                state,
                ManagerMessage::MoveSettingsGlobalMountsSelection {
                    delta: 1,
                    term: state.cached_term_size,
                    footer_h,
                },
            );
            return;
        }
        KeyCode::Char('r' | 'R') => {
            dispatch_manager(state, ManagerMessage::ToggleSettingsGlobalMountReadonly);
            return;
        }
        _ => {}
    }

    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    let is_dirty = settings.is_dirty();
    let global = &mut settings.mounts;
    let mut return_to_list = false;
    match key.code {
        KeyCode::Esc | KeyCode::Char('q' | 'Q') => {
            if is_dirty {
                global.modal = Some(confirm_modal(GlobalMountConfirm::Discard));
            } else {
                return_to_list = true;
            }
        }
        KeyCode::Enter if global.selected == global.pending.len() => {
            open_global_mount_scope_picker(global);
        }
        KeyCode::Char('a' | 'A') => {
            open_global_mount_scope_picker(global);
        }
        // S is handled before the match (early-return above) so `open_settings_save_preview`
        // can receive all of `settings` without conflicting with the `global` borrow.
        KeyCode::Char('d' | 'D') if !global.pending.is_empty() => {
            global.modal = Some(confirm_modal(GlobalMountConfirm::Remove));
        }
        KeyCode::Char('o' | 'O') => {
            if let Some(row) = global.pending.get(global.selected) {
                if let Some(web_url) = global.mount_info_cache.github_web_url(&row.mount.src) {
                    state.request_effect(ManagerEffect::OpenUrl(web_url));
                } else {
                    global.error = Some(global_mount_no_github_url_message().into());
                }
            }
        }
        KeyCode::Char('n' | 'N') => open_edit_text(state, GlobalMountTextTarget::Rename),
        KeyCode::Char('1') => open_edit_text(state, GlobalMountTextTarget::Source),
        KeyCode::Char('2') => open_edit_text(state, GlobalMountTextTarget::Destination),
        KeyCode::Char('3') => open_edit_text(state, GlobalMountTextTarget::Scope),
        _ => {}
    }
    if return_to_list {
        dispatch_manager(state, ManagerMessage::ReturnToList);
    }
}

fn handle_env_key(state: &mut ManagerState<'_>, key: KeyEvent) {
    let op_cache = state.op_cache.clone();
    let op_available = state.op_available;
    let term_size = state.cached_term_size;
    let ManagerStage::Settings(settings) = &state.stage else {
        return;
    };
    let footer_h = settings.cached_footer_h;
    match key.code {
        KeyCode::Up | KeyCode::Char('k' | 'K') => {
            dispatch_manager(
                state,
                ManagerMessage::MoveSettingsEnvSelection {
                    delta: -1,
                    term: term_size,
                    footer_h,
                },
            );
            return;
        }
        KeyCode::Down | KeyCode::Char('j' | 'J') => {
            dispatch_manager(
                state,
                ManagerMessage::MoveSettingsEnvSelection {
                    delta: 1,
                    term: term_size,
                    footer_h,
                },
            );
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
        KeyCode::Char('a' | 'A') => {
            open_settings_env_add_modal(settings);
        }
        KeyCode::Char('s' | 'S') => {
            open_settings_save_preview(settings);
        }
        KeyCode::Char('d' | 'D') if (key.modifiers - KeyModifiers::SHIFT).is_empty() => {
            open_settings_env_delete_confirm(settings);
        }
        KeyCode::Char('m' | 'M') if (key.modifiers - KeyModifiers::SHIFT).is_empty() => {
            toggle_settings_env_mask(settings);
        }
        KeyCode::Char('p' | 'P')
            if (key.modifiers - KeyModifiers::SHIFT).is_empty() && op_available =>
        {
            open_settings_env_picker_modal(settings, op_cache);
        }
        KeyCode::Enter => {
            // For op-ref rows Enter re-opens the 1Password picker (same as P).
            // W3C rule: Enter = action/activate; op-ref rows open the picker.
            let rows = settings_env_flat_rows(settings);
            let is_op_ref = matches!(
                rows.get(settings.env.selected),
                Some(SettingsEnvRow::Key { scope, key })
                    if settings_update::settings_env_value(&settings.env.pending, scope, key)
                        .is_some_and(|v| matches!(v, crate::operator_env::EnvValue::OpRef(_)))
            );
            if is_op_ref && op_available {
                open_settings_env_picker_modal(settings, op_cache);
            } else {
                open_settings_env_enter_modal(settings);
            }
        }
        _ => {}
    }
    if return_to_list {
        dispatch_manager(state, ManagerMessage::ReturnToList);
    }
}

fn handle_auth_key(state: &mut ManagerState<'_>, key: KeyEvent) {
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
            open_settings_auth_form(&mut settings.auth, &settings.env);
        }
        KeyCode::Char('s' | 'S') => {
            open_settings_save_preview(settings);
        }
        _ => {}
    }
    if return_to_list {
        dispatch_manager(state, ManagerMessage::ReturnToList);
    }
}

fn open_settings_auth_form(
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
    let form = AuthForm::from_existing(kind, row.mode, existing_credential);
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
pub fn settings_auth_can_generate_token(auth: &crate::console::tui::state::SettingsAuthState) -> bool {
    matches!(
        auth.modal.as_ref(),
        Some(SettingsAuthModal::AuthForm { state, .. })
            if can_generate_claude_oauth_token(state.kind, state.mode)
    )
}

#[allow(clippy::too_many_lines)]
pub(super) fn handle_settings_auth_modal(
    auth: &mut crate::console::tui::state::SettingsAuthState,
    env: &mut crate::console::tui::state::SettingsEnvState<'_>,
    pending_token_generate: &mut Option<crate::console::tui::state::PendingTokenGenerate>,
    key: KeyEvent,
    op_available: bool,
    op_cache: std::rc::Rc<std::cell::RefCell<crate::operator_env::OpCache>>,
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
            let plan = auth_form_key_plan(
                *focus,
                key.code,
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
                        *pending_token_generate = Some(crate::console::tui::state::PendingTokenGenerate {
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
                            crate::console::tui::op_picker::OpPickerState::new_with_cache(
                                op_cache,
                            ),
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
    state.set_literal(value.to_string());
    auth.modal = Some(SettingsAuthModal::AuthForm {
        target,
        state,
        focus: AuthFormFocus::Save,
        literal_buffer: value.to_string(),
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
fn apply_op_picker_to_settings_auth_form_with_runner<R: crate::operator_env::OpRunner + ?Sized>(
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
        Ok(_) => {
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
        focus: crate::console::tui::state::AuthFormFocus::Save,
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
    }
    clear_settings_auth_env_values(*kind, &mut auth.github_env, &mut env.pending.env);
}

fn handle_general_key(state: &mut ManagerState<'_>, key: KeyEvent) {
    match key.code {
        KeyCode::Up | KeyCode::Char('k' | 'K') => {
            dispatch_manager(
                state,
                ManagerMessage::MoveSettingsGeneralSelection { delta: -1 },
            );
            return;
        }
        KeyCode::Down | KeyCode::Char('j' | 'J') => {
            dispatch_manager(
                state,
                ManagerMessage::MoveSettingsGeneralSelection { delta: 1 },
            );
            return;
        }
        KeyCode::Char(' ') => {
            dispatch_manager(state, ManagerMessage::ToggleSettingsGeneralSelected);
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
        KeyCode::Char('s' | 'S') => {
            open_settings_save_preview(settings);
        }
        _ => {}
    }
    if return_to_list {
        dispatch_manager(state, ManagerMessage::ReturnToList);
    }
}

fn handle_trust_key(state: &mut ManagerState<'_>, key: KeyEvent) {
    let term_size = state.cached_term_size;
    let term_width = term_size.width;
    let ManagerStage::Settings(settings) = &state.stage else {
        return;
    };
    let footer_h = settings.cached_footer_h;
    let content_width =
        jackin_console::tui::screens::settings::update::trust_content_width(&settings.trust);
    match key.code {
        KeyCode::Up | KeyCode::Char('k' | 'K') => {
            dispatch_manager(
                state,
                ManagerMessage::MoveSettingsTrustSelection {
                    delta: -1,
                    term: term_size,
                    footer_h,
                },
            );
            return;
        }
        KeyCode::Down | KeyCode::Char('j' | 'J') => {
            dispatch_manager(
                state,
                ManagerMessage::MoveSettingsTrustSelection {
                    delta: 1,
                    term: term_size,
                    footer_h,
                },
            );
            return;
        }
        KeyCode::Char('h' | 'H') => {
            dispatch_manager(
                state,
                ManagerMessage::ScrollSettingsTrustHorizontal {
                    delta: -8,
                    term_width,
                    content_width,
                },
            );
            return;
        }
        KeyCode::Char('l' | 'L') => {
            dispatch_manager(
                state,
                ManagerMessage::ScrollSettingsTrustHorizontal {
                    delta: 8,
                    term_width,
                    content_width,
                },
            );
            return;
        }
        KeyCode::Char(' ') => {
            dispatch_manager(state, ManagerMessage::ToggleSettingsTrustSelected);
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
        KeyCode::Char('s' | 'S') => {
            open_settings_save_preview(settings);
        }
        _ => {}
    }
    if return_to_list {
        dispatch_manager(state, ManagerMessage::ReturnToList);
    }
}

#[allow(clippy::too_many_lines)]
pub(super) fn handle_settings_confirm_modal(
    settings: &mut crate::console::tui::state::SettingsState<'_>,
    key: KeyEvent,
) -> SettingsModalOutcome {
    let Some(modal) = settings.mounts.modal.take() else {
        return SettingsModalOutcome::Continue;
    };
    let mut outcome = SettingsModalOutcome::Continue;
    match modal {
        GlobalMountModal::Text { target, mut state } => match state.handle_key(key) {
            ModalOutcome::Commit(value) => {
                let committed_target = target.clone();
                settings.mounts.modal = Some(GlobalMountModal::Text { target, state });
                outcome = commit_text(&mut settings.mounts, &committed_target, &value);
            }
            ModalOutcome::Cancel => {
                settings.mounts.pop_modal_chain();
                if settings.mounts.modal.is_none() && settings.mounts.add_draft.take().is_some() {
                    settings.mounts.error = Some(global_mount_add_cancelled_message().to_string());
                }
            }
            ModalOutcome::Continue => {
                settings.mounts.modal = Some(GlobalMountModal::Text { target, state });
            }
        },
        GlobalMountModal::FileBrowser { mut state } => {
            let browser_outcome = state.handle_key(key);
            match browser_outcome {
                FileBrowserOutcome::Cancel => {
                    settings.mounts.pop_modal_chain();
                    if settings.mounts.modal.is_none() {
                        settings.mounts.add_draft = None;
                    }
                }
                FileBrowserOutcome::ResolveGitUrl(path) => {
                    settings.mounts.modal = Some(GlobalMountModal::FileBrowser { state });
                    outcome = SettingsModalOutcome::ResolveFileBrowserGitUrl(path);
                }
                FileBrowserOutcome::OpenGitUrl(url) => {
                    settings.mounts.modal = Some(GlobalMountModal::FileBrowser { state });
                    outcome = SettingsModalOutcome::OpenUrl(url);
                }
                FileBrowserOutcome::Continue => {
                    settings.mounts.modal = Some(GlobalMountModal::FileBrowser { state });
                }
                FileBrowserOutcome::Commit(_)
                | FileBrowserOutcome::NavigateTo(_)
                | FileBrowserOutcome::NavigateUp
                | FileBrowserOutcome::RequestCommit(_) => {
                    settings.mounts.modal = Some(GlobalMountModal::FileBrowser { state });
                    outcome = SettingsModalOutcome::ApplyFileBrowserOutcome(browser_outcome);
                }
            }
        }
        GlobalMountModal::MountDstChoice { mut state } => {
            use jackin_console::tui::components::mount_dst_choice::MountDstChoice;
            let src = state.src.clone();
            match state.handle_key(key) {
                ModalOutcome::Commit(MountDstChoice::SamePath) => {
                    if let Some(draft) = settings.mounts.add_draft.as_mut() {
                        draft.dst = src;
                    }
                    finalize_global_mount_add(&mut settings.mounts);
                }
                ModalOutcome::Commit(MountDstChoice::Edit) => {
                    if let Some(draft) = settings.mounts.add_draft.as_mut() {
                        draft.dst.clone_from(&src);
                    }
                    settings.mounts.modal = Some(GlobalMountModal::MountDstChoice { state });
                    settings
                        .mounts
                        .open_sub_modal(text_modal_for_target(
                            GlobalMountTextTarget::AddDestination,
                            &src,
                        ));
                }
                ModalOutcome::Cancel => {
                    settings.mounts.pop_modal_chain();
                    if settings.mounts.modal.is_none() {
                        settings.mounts.add_draft = None;
                    }
                }
                ModalOutcome::Continue => {
                    settings.mounts.modal = Some(GlobalMountModal::MountDstChoice { state });
                }
            }
        }
        GlobalMountModal::ScopePicker { mut state } => match state.handle_key(key) {
            ModalOutcome::Commit(choice) => {
                // Drop the picker before dispatching: commit_text
                // (AllAgents path) calls clear_modal_chain anyway, and
                // open_sub_modal (SpecificAgent → RolePicker) would
                // otherwise stash this already-committed picker as
                // the RolePicker's parent — Esc on RolePicker would
                // then resurrect a consumed ScopePicker.
                outcome = commit_add_scope_choice(settings, choice);
            }
            ModalOutcome::Cancel => {
                settings.mounts.pop_modal_chain();
                if settings.mounts.modal.is_none() && settings.mounts.add_draft.take().is_some() {
                    settings.mounts.error = Some(global_mount_add_cancelled_message().to_string());
                }
            }
            ModalOutcome::Continue => {
                settings.mounts.modal = Some(GlobalMountModal::ScopePicker { state });
            }
        },
        GlobalMountModal::RolePicker { state: mut picker } => match picker.handle_key(key) {
            ModalOutcome::Commit(role) => {
                if let Some(draft) = settings.mounts.add_draft.as_mut() {
                    draft.scope = Some(role.key());
                    settings.mounts.modal = Some(GlobalMountModal::RolePicker { state: picker });
                    outcome = SettingsModalOutcome::OpenGlobalMountFileBrowser;
                } else {
                    settings.mounts.error = Some(global_mount_add_draft_lost_message().into());
                }
            }
            ModalOutcome::Cancel => {
                settings.mounts.pop_modal_chain();
                if settings.mounts.modal.is_none() && settings.mounts.add_draft.take().is_some() {
                    settings.mounts.error = Some(global_mount_add_cancelled_message().to_string());
                }
            }
            ModalOutcome::Continue => {
                settings.mounts.modal = Some(GlobalMountModal::RolePicker { state: picker });
            }
        },
        GlobalMountModal::Confirm { action, mut state } => {
            match settings_update::settings_confirm_plan(action, state.handle_key(key)) {
                settings_update::SettingsConfirmPlan::Commit => {
                    outcome = commit_settings_confirm(settings, action);
                }
                settings_update::SettingsConfirmPlan::Cancel { abort_sensitive } => {
                    if abort_sensitive {
                        settings.mounts.error =
                            Some(settings_sensitive_paths_not_confirmed_message().into());
                    }
                    settings.mounts.clear_modal_chain();
                }
                settings_update::SettingsConfirmPlan::Continue => {
                    settings.mounts.modal = Some(GlobalMountModal::Confirm { action, state });
                }
            }
        }
        GlobalMountModal::PreviewSave { mut state } => match state.handle_key(key) {
            ModalOutcome::Commit(_) => {
                outcome = request_settings_save(settings);
            }
            ModalOutcome::Cancel => settings.mounts.clear_modal_chain(),
            ModalOutcome::Continue => {
                settings.mounts.modal = Some(GlobalMountModal::PreviewSave { state });
            }
        },
    }
    outcome
}

#[allow(clippy::too_many_lines)]
pub(super) fn handle_settings_env_modal(
    env: &mut crate::console::tui::state::SettingsEnvState<'_>,
    key: KeyEvent,
    op_cache: std::rc::Rc<std::cell::RefCell<crate::operator_env::OpCache>>,
) {
    let Some(modal) = env.modal.take() else {
        return;
    };
    match modal {
        SettingsEnvModal::Text { target, mut state } => match state.handle_key(key) {
            ModalOutcome::Commit(value) => {
                let committed_target = target.clone();
                env.modal = Some(SettingsEnvModal::Text { target, state });
                commit_env_text(env, &committed_target, &value);
            }
            ModalOutcome::Cancel => {
                env.pop_modal_chain();
                if env.modal.is_none() {
                    env.pending_env_key = None;
                    env.pending_picker_value = None;
                    env.error = Some(settings_env_edit_cancelled_message().to_string());
                }
            }
            ModalOutcome::Continue => {
                env.modal = Some(SettingsEnvModal::Text { target, state });
            }
        },
        SettingsEnvModal::SourcePicker { state: mut source } => {
            use jackin_console::tui::components::source_picker::SourceChoice;
            match source.handle_key(key) {
                ModalOutcome::Commit(SourceChoice::Plain) => {
                    let Some((scope, key)) = env.pending_env_key.clone() else {
                        env.clear_modal_chain();
                        return;
                    };
                    env.modal = Some(SettingsEnvModal::SourcePicker { state: source });
                    env.open_sub_modal(env_text_modal(
                        SettingsEnvTextTarget::EnvValue {
                            scope,
                            key: key.clone(),
                        },
                        &settings_env_value_text_label(&key),
                        "",
                    ));
                }
                ModalOutcome::Commit(SourceChoice::Op) => {
                    let Some((scope, key)) = env.pending_env_key.clone() else {
                        env.clear_modal_chain();
                        return;
                    };
                    env.pending_picker_target = Some((scope, Some(key)));
                    env.pending_env_key = None;
                    env.modal = Some(SettingsEnvModal::SourcePicker { state: source });
                    env.open_sub_modal(SettingsEnvModal::OpPicker {
                        state: Box::new(
                            crate::console::tui::op_picker::OpPickerState::new_with_cache(
                                op_cache,
                            ),
                        ),
                    });
                }
                ModalOutcome::Cancel => {
                    env.pop_modal_chain();
                    env.pending_env_key = None;
                    env.pending_picker_value = None;
                }
                ModalOutcome::Continue => {
                    env.modal = Some(SettingsEnvModal::SourcePicker { state: source });
                }
            }
        }
        SettingsEnvModal::OpPicker { state: mut picker } => match picker.handle_key(key) {
            // Browse-mode caller: only `Existing` is reachable.
            ModalOutcome::Commit(
                crate::console::tui::op_picker::OpPickerSelection::NewItem { .. }
                | crate::console::tui::op_picker::OpPickerSelection::EditItemField { .. },
            ) => unreachable!("settings-env OpPicker runs in Browse mode"),
            ModalOutcome::Commit(
                crate::console::tui::op_picker::OpPickerSelection::Existing(op_ref),
            ) => {
                let target = env.pending_picker_target.take();
                match target {
                    Some((scope, Some(key))) => {
                        set_settings_env_value_typed(
                            env,
                            &scope,
                            &key,
                            crate::operator_env::EnvValue::OpRef(op_ref),
                        );
                        env.clear_modal_chain();
                    }
                    Some((scope, None)) => {
                        env.pending_picker_value =
                            Some(crate::operator_env::EnvValue::OpRef(op_ref));
                        let state = settings_env_key_input_state(
                            &env.pending,
                            &scope,
                            settings_env_new_key_after_picker_label(&scope),
                            "",
                        );
                        env.modal = Some(SettingsEnvModal::OpPicker { state: picker });
                        env.open_sub_modal(SettingsEnvModal::Text {
                            target: SettingsEnvTextTarget::EnvKey { scope },
                            state: Box::new(state),
                        });
                    }
                    None => env.clear_modal_chain(),
                }
            }
            ModalOutcome::Cancel => {
                env.pop_modal_chain();
                env.pending_picker_target = None;
                env.pending_picker_value = None;
            }
            ModalOutcome::Continue => {
                env.modal = Some(SettingsEnvModal::OpPicker { state: picker });
            }
        },
        SettingsEnvModal::RolePicker { state: mut picker } => match picker.handle_key(key) {
            ModalOutcome::Commit(role) => {
                let role_key = role.key();
                let scope = SettingsEnvScope::Role(role_key.clone());
                let state = settings_env_key_input_state(
                    &env.pending,
                    &scope,
                    settings_env_new_key_label(&scope),
                    "",
                );
                env.modal = Some(SettingsEnvModal::RolePicker { state: picker });
                env.open_sub_modal(SettingsEnvModal::Text {
                    target: SettingsEnvTextTarget::EnvKey { scope },
                    state: Box::new(state),
                });
            }
            ModalOutcome::Cancel => {
                env.pop_modal_chain();
                if env.modal.is_none() {
                    env.error = Some(settings_env_add_cancelled_message().to_string());
                }
            }
            ModalOutcome::Continue => {
                env.modal = Some(SettingsEnvModal::RolePicker { state: picker });
            }
        },
        SettingsEnvModal::ScopePicker { mut state } => match state.handle_key(key) {
            ModalOutcome::Commit(choice) => match choice {
                jackin_console::tui::components::scope_picker::ScopeChoice::AllAgents => {
                    let scope = SettingsEnvScope::Global;
                    let input_state = settings_env_key_input_state(
                        &env.pending,
                        &scope,
                        settings_env_new_key_label(&scope),
                        "",
                    );
                    // Don't stash the just-committed ScopePicker as
                    // the Text modal's parent — Esc on Text would
                    // pop back into a consumed picker. Start the
                    // child modal with an empty parent chain.
                    env.open_sub_modal(SettingsEnvModal::Text {
                        target: SettingsEnvTextTarget::EnvKey { scope },
                        state: Box::new(input_state),
                    });
                }
                jackin_console::tui::components::scope_picker::ScopeChoice::SpecificAgent => {
                    open_settings_env_role_picker(env);
                }
            },
            ModalOutcome::Cancel => {
                env.pop_modal_chain();
                if env.modal.is_none() {
                    env.error = Some(settings_env_add_cancelled_message().to_string());
                }
            }
            ModalOutcome::Continue => {
                env.modal = Some(SettingsEnvModal::ScopePicker { state });
            }
        },
        SettingsEnvModal::Confirm { action, mut state } => match state.handle_key(key) {
            ModalOutcome::Commit(true) => match action {
                SettingsEnvConfirm::Delete => {
                    delete_selected_settings_env(env);
                    env.clear_modal_chain();
                }
            },
            ModalOutcome::Commit(false) | ModalOutcome::Cancel => env.clear_modal_chain(),
            ModalOutcome::Continue => {
                env.modal = Some(SettingsEnvModal::Confirm { action, state });
            }
        },
    }
}

fn commit_settings_confirm(
    settings: &mut crate::console::tui::state::SettingsState<'_>,
    action: GlobalMountConfirm,
) -> SettingsModalOutcome {
    match action {
        GlobalMountConfirm::Remove => {
            let global = &mut settings.mounts;
            if global.selected < global.pending.len() {
                global.pending.remove(global.selected);
                global.selected = global.selected.min(global.pending.len());
            }
            SettingsModalOutcome::Continue
        }
        GlobalMountConfirm::Save => request_settings_save(settings),
        GlobalMountConfirm::Sensitive => {
            open_settings_save_preview(settings);
            SettingsModalOutcome::Continue
        }
        GlobalMountConfirm::Discard => {
            settings.discard();
            settings.mounts.exit_requested = true;
            SettingsModalOutcome::Continue
        }
    }
}

fn request_settings_save(
    settings: &mut crate::console::tui::state::SettingsState<'_>,
) -> SettingsModalOutcome {
    settings.remove_zai_key_when_auth_ignored();
    SettingsModalOutcome::SaveSettings
}

fn open_settings_save_preview(settings: &mut crate::console::tui::state::SettingsState<'_>) {
    let lines = super::save::build_settings_save_lines(settings);
    settings.mounts.modal = Some(crate::console::tui::state::GlobalMountModal::PreviewSave {
        state: jackin_console::tui::components::confirm_save::ConfirmSaveState::new(lines),
    });
}

fn commit_text(
    global: &mut crate::console::tui::state::GlobalMountsState<'_>,
    target: &GlobalMountTextTarget,
    value: &str,
) -> SettingsModalOutcome {
    let trimmed = value.trim();
    match target {
        GlobalMountTextTarget::AddScope => {
            return commit_add_scope_text(global, trimmed);
        }
        GlobalMountTextTarget::AddName => {
            commit_add_name_text(global, trimmed);
        }
        GlobalMountTextTarget::AddSource => {
            commit_add_source_text(global, trimmed);
        }
        GlobalMountTextTarget::AddDestination => {
            commit_add_destination_text(global, trimmed);
        }
        GlobalMountTextTarget::Source => {
            let Some(row) = global.pending.get_mut(global.selected) else {
                global.error = Some(global_mount_gone_message().into());
                return SettingsModalOutcome::Continue;
            };
            row.mount.src = resolve_path(trimmed);
            global.clear_modal_chain();
        }
        GlobalMountTextTarget::Destination => {
            let Some(row) = global.pending.get_mut(global.selected) else {
                global.error = Some(global_mount_gone_message().into());
                return SettingsModalOutcome::Continue;
            };
            row.mount.dst = trimmed.to_string();
            global.clear_modal_chain();
        }
        GlobalMountTextTarget::Scope => {
            let Some(row) = global.pending.get_mut(global.selected) else {
                global.error = Some(global_mount_gone_message().into());
                return SettingsModalOutcome::Continue;
            };
            row.scope = crate::console::domain::global_mount_scope_value(trimmed);
            global.clear_modal_chain();
        }
        GlobalMountTextTarget::Rename => {
            if trimmed.is_empty() {
                global.error = Some(global_mount_name_empty_message().into());
                return SettingsModalOutcome::Continue;
            }
            let Some(row) = global.pending.get_mut(global.selected) else {
                global.error = Some(global_mount_gone_message().into());
                return SettingsModalOutcome::Continue;
            };
            row.name = trimmed.to_string();
            global.clear_modal_chain();
        }
    }
    SettingsModalOutcome::Continue
}

fn commit_env_text(
    env: &mut crate::console::tui::state::SettingsEnvState<'_>,
    target: &SettingsEnvTextTarget,
    value: &str,
) {
    let trimmed = value.trim();
    match target {
        SettingsEnvTextTarget::EnvKey { scope } => {
            if trimmed.is_empty() {
                env.error = Some(settings_env_empty_key_error_message().into());
                let state = settings_env_key_input_state(
                    &env.pending,
                    scope,
                    settings_env_empty_key_label(),
                    "",
                );
                env.modal = Some(SettingsEnvModal::Text {
                    target: SettingsEnvTextTarget::EnvKey {
                        scope: scope.clone(),
                    },
                    state: Box::new(state),
                });
                return;
            }
            let key = trimmed.to_string();
            if let Some(stashed) = env.pending_picker_value.take() {
                set_settings_env_value_typed(env, scope, &key, stashed);
                env.pending_env_key = None;
                env.clear_modal_chain();
                return;
            }
            env.pending_env_key = Some((scope.clone(), key.clone()));
            env.open_sub_modal(SettingsEnvModal::SourcePicker {
                state: settings_env_source_picker_state(key),
            });
        }
        SettingsEnvTextTarget::EnvValue { scope, key } => {
            set_settings_env_value_typed(
                env,
                scope,
                key,
                crate::operator_env::EnvValue::Plain(value.to_string()),
            );
            env.pending_env_key = None;
            env.clear_modal_chain();
        }
    }
}

fn open_settings_env_role_picker(env: &mut crate::console::tui::state::SettingsEnvState<'_>) {
    use crate::selector::RolePickerState;
    use crate::selector::RoleSelector;

    let roles = env
        .pending
        .roles
        .keys()
        .filter_map(|role| RoleSelector::parse(role).ok())
        .collect::<Vec<_>>();
    if roles.is_empty() {
        env.error = Some(settings_no_registered_roles_error_message().into());
        return;
    }
    env.open_sub_modal(SettingsEnvModal::RolePicker {
        state: RolePickerState::new(roles),
    });
}

fn commit_add_scope_text(
    global: &mut crate::console::tui::state::GlobalMountsState<'_>,
    value: &str,
) -> SettingsModalOutcome {
    let Some(draft) = global.add_draft.as_mut() else {
        global.error = Some(global_mount_add_draft_lost_message().into());
        return SettingsModalOutcome::Continue;
    };
    draft.scope = crate::console::domain::global_mount_scope_value(value);
    SettingsModalOutcome::OpenGlobalMountFileBrowser
}

fn commit_add_name_text(global: &mut crate::console::tui::state::GlobalMountsState<'_>, value: &str) {
    if value.is_empty() {
        global.error = Some(global_mount_name_empty_message().into());
        global.modal = Some(text_modal_for_target(GlobalMountTextTarget::AddName, ""));
        return;
    }
    let Some(draft) = global.add_draft.as_mut() else {
        global.error = Some(global_mount_add_draft_lost_message().into());
        return;
    };
    draft.name = value.to_string();
    global.open_sub_modal(text_modal_for_target(GlobalMountTextTarget::AddSource, ""));
}

fn commit_add_source_text(global: &mut crate::console::tui::state::GlobalMountsState<'_>, value: &str) {
    let Some(draft) = global.add_draft.as_mut() else {
        global.error = Some(global_mount_add_draft_lost_message().into());
        return;
    };
    draft.src = resolve_path(value);
    global.open_sub_modal(text_modal_for_target(
        GlobalMountTextTarget::AddDestination,
        "",
    ));
}

fn commit_add_destination_text(
    global: &mut crate::console::tui::state::GlobalMountsState<'_>,
    value: &str,
) {
    let Some(draft) = global.add_draft.as_mut() else {
        global.error = Some(global_mount_add_draft_lost_message().into());
        return;
    };
    draft.dst = value.to_string();
    finalize_global_mount_add(global);
}

fn open_global_mount_scope_picker(global: &mut crate::console::tui::state::GlobalMountsState<'_>) {
    global.add_draft = Some(GlobalMountDraft::default());
    global.modal_parents.clear();
    global.modal = Some(scope_picker_modal());
}

fn finalize_global_mount_add(global: &mut crate::console::tui::state::GlobalMountsState<'_>) {
    let Some(mut draft) = global.add_draft.take() else {
        global.error = Some(global_mount_add_draft_lost_message().into());
        return;
    };
    if draft.dst.trim().is_empty() {
        global.error = Some(global_mount_destination_empty_message().into());
        global.add_draft = Some(draft);
        return;
    }
    draft.name = crate::console::domain::unique_global_mount_name(
        &global.pending,
        draft.scope.as_deref(),
        &draft.dst,
    );
    global.pending.push(crate::config::GlobalMountRow {
        scope: draft.scope,
        name: draft.name,
        mount: crate::console::domain::shared_mount_config(draft.src, draft.dst, false),
    });
    global.selected = global.pending.len().saturating_sub(1);
    global.clear_modal_chain();
}

fn open_edit_text(state: &mut ManagerState<'_>, target: GlobalMountTextTarget) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    let global = &mut settings.mounts;
    let Some(row) = global.pending.get(global.selected) else {
        return;
    };
    let initial = match target {
        GlobalMountTextTarget::Rename => row.name.clone(),
        GlobalMountTextTarget::Source => row.mount.src.clone(),
        GlobalMountTextTarget::Destination => row.mount.dst.clone(),
        GlobalMountTextTarget::Scope => global_mount_scope_text_value(row.scope.as_deref()),
        // Add-flow targets are driven by the four-step text wizard, not this entry point.
        GlobalMountTextTarget::AddScope
        | GlobalMountTextTarget::AddName
        | GlobalMountTextTarget::AddSource
        | GlobalMountTextTarget::AddDestination => return,
    };
    let Some(label) = global_mount_text_target_label(&target) else {
        return;
    };
    global.modal = Some(text_modal(target, label, &initial));
}

fn open_settings_env_enter_modal(settings: &mut crate::console::tui::state::SettingsState<'_>) {
    let rows = settings_env_flat_rows(settings);
    let plan = settings_update::settings_env_enter_plan_for_row(
        &settings.env.pending,
        rows.get(settings.env.selected),
        |value| !value.is_some_and(|v| matches!(v, crate::operator_env::EnvValue::OpRef(_))),
    );
    match plan {
        SettingsEnvEnterPlan::EditValue { scope, key } => {
            let value = settings_update::settings_env_value(&settings.env.pending, &scope, &key);
            let current = settings_env_value_current_text(value.map(|v| v.as_persisted_str()));
            let target = SettingsEnvTextTarget::EnvValue {
                scope,
                key: key.clone(),
            };
            let state =
                settings_env_text_input_state(&target, settings_env_value_text_label(&key), current);
            settings.env.modal = Some(SettingsEnvModal::Text {
                target,
                state: Box::new(state),
            });
        }
        SettingsEnvEnterPlan::OpenScopePicker => {
            settings.env.modal = Some(SettingsEnvModal::ScopePicker {
                state: settings_env_scope_picker_state(),
            });
        }
        SettingsEnvEnterPlan::ExpandRole(role) => {
            settings.env.expanded.insert(role);
        }
        SettingsEnvEnterPlan::AddRoleKey { scope } => {
            let label = settings_env_new_key_label(&scope);
            let state = settings_env_key_input_state(&settings.env.pending, &scope, label, "");
            settings.env.modal = Some(SettingsEnvModal::Text {
                target: SettingsEnvTextTarget::EnvKey { scope },
                state: Box::new(state),
            });
        }
        SettingsEnvEnterPlan::Noop => {}
    }
}

fn open_settings_env_add_modal(settings: &mut crate::console::tui::state::SettingsState<'_>) {
    let rows = settings_env_flat_rows(settings);
    let Some(scope) = settings_update::settings_env_add_target_for_row(rows.get(settings.env.selected))
    else {
        return;
    };
    let label = settings_env_new_key_label(&scope);
    let state = settings_env_key_input_state(&settings.env.pending, &scope, label, "");
    settings.env.modal = Some(SettingsEnvModal::Text {
        target: SettingsEnvTextTarget::EnvKey { scope },
        state: Box::new(state),
    });
}

fn open_settings_env_delete_confirm(settings: &mut crate::console::tui::state::SettingsState<'_>) {
    let rows = settings_env_flat_rows(settings);
    let Some(SettingsEnvRow::Key { key, .. }) = rows.get(settings.env.selected).cloned() else {
        return;
    };
    settings.env.modal = Some(SettingsEnvModal::Confirm {
        action: SettingsEnvConfirm::Delete,
        state: settings_env_delete_confirm_state(&key),
    });
}

fn toggle_settings_env_mask(settings: &mut crate::console::tui::state::SettingsState<'_>) {
    let rows = settings_env_flat_rows(settings);
    settings_update::toggle_settings_env_mask_for_row(
        &mut settings.env.unmasked_rows,
        &settings.env.pending,
        rows.get(settings.env.selected),
        |value| !matches!(value, crate::operator_env::EnvValue::OpRef(_)),
    );
}

fn open_settings_env_picker_modal(
    settings: &mut crate::console::tui::state::SettingsState<'_>,
    op_cache: std::rc::Rc<std::cell::RefCell<crate::operator_env::OpCache>>,
) {
    let rows = settings_env_flat_rows(settings);
    let Some(target) =
        settings_update::settings_env_picker_target_for_row(rows.get(settings.env.selected))
    else {
        return;
    };
    settings.env.pending_picker_target = Some(target);
    settings.env.modal = Some(SettingsEnvModal::OpPicker {
        state: Box::new(
            crate::console::tui::op_picker::OpPickerState::new_with_cache(op_cache),
        ),
    });
}

fn delete_selected_settings_env(env: &mut crate::console::tui::state::SettingsEnvState<'_>) {
    let rows = settings_env_state_flat_rows(env);
    let selected = env.selected;
    settings_update::remove_settings_env_row(
        &mut env.pending,
        &env.expanded,
        &mut env.selected,
        rows.get(selected),
    );
}

fn set_settings_env_value_typed(
    env: &mut crate::console::tui::state::SettingsEnvState<'_>,
    scope: &SettingsEnvScope,
    key: &str,
    value: crate::operator_env::EnvValue,
) {
    settings_update::set_settings_env_value(
        &mut env.pending,
        &mut env.expanded,
        scope,
        key,
        value,
    );
}

/// Promote any pending error from a settings sub-tab to `settings.error_popup`,
/// pop back to the workspace list when a handler set `exit_requested`.
pub(super) fn after_settings_event(state: &mut ManagerState<'_>) {
    let (exit, error) = {
        let ManagerStage::Settings(settings) = &mut state.stage else {
            return;
        };
        // Each tab dispatches to exactly one sub-handler per keypress, so at
        // most one error field is set at a time — `or_else` laziness is safe.
        let error = settings
            .mounts
            .error
            .take()
            .or_else(|| settings.env.error.take())
            .or_else(|| settings.auth.error.take())
            .or_else(|| settings.trust.error.take());
        let exit = std::mem::take(&mut settings.mounts.exit_requested);
        (exit, error)
    };
    if let Some(msg) = error {
        dispatch_manager(
            state,
            ManagerMessage::OpenSettingsErrorPopup {
                title: settings_error_popup_title().into(),
                message: msg,
            },
        );
    }
    if exit {
        dispatch_manager(state, ManagerMessage::ReturnToList);
    }
}

fn confirm_modal(action: GlobalMountConfirm) -> GlobalMountModal<'static> {
    GlobalMountModal::Confirm {
        action,
        state: global_mount_confirm_state(action),
    }
}

fn scope_picker_modal() -> GlobalMountModal<'static> {
    GlobalMountModal::ScopePicker {
        state: global_mount_scope_picker_state(),
    }
}

fn commit_add_scope_choice(
    settings: &mut crate::console::tui::state::SettingsState<'_>,
    choice: jackin_console::tui::components::scope_picker::ScopeChoice,
) -> SettingsModalOutcome {
    match choice {
        jackin_console::tui::components::scope_picker::ScopeChoice::AllAgents => {
            commit_text(&mut settings.mounts, &GlobalMountTextTarget::AddScope, "")
        }
        jackin_console::tui::components::scope_picker::ScopeChoice::SpecificAgent => {
            open_global_mount_role_picker(settings);
            SettingsModalOutcome::Continue
        }
    }
}

fn open_global_mount_role_picker(settings: &mut crate::console::tui::state::SettingsState<'_>) {
    let roles = settings
        .trust
        .pending
        .iter()
        .filter_map(|row| RoleSelector::parse(&row.role).ok())
        .collect::<Vec<_>>();
    if roles.is_empty() {
        settings.mounts.error = Some(settings_no_registered_roles_error_message().into());
        return;
    }
    settings
        .mounts
        .open_sub_modal(GlobalMountModal::RolePicker {
            state: RolePickerState::new(roles),
        });
}

fn text_modal(
    target: GlobalMountTextTarget,
    label: &str,
    initial: &str,
) -> GlobalMountModal<'static> {
    GlobalMountModal::Text {
        target,
        state: Box::new(global_mount_text_input_state(label, initial)),
    }
}

fn text_modal_for_target(target: GlobalMountTextTarget, initial: &str) -> GlobalMountModal<'static> {
    let label = global_mount_text_target_label(&target).unwrap_or("Value");
    text_modal(target, label, initial)
}

fn env_text_modal(
    target: SettingsEnvTextTarget,
    label: &str,
    initial: &str,
) -> SettingsEnvModal<'static> {
    let state = settings_env_text_input_state(&target, label, initial);
    SettingsEnvModal::Text {
        target,
        state: Box::new(state),
    }
}

#[cfg(test)]
mod tests {
    use crate::console::tui::state::{
        ManagerStage, ManagerState, SettingsEnvModal, SettingsEnvTextTarget, SettingsState,
        SettingsTab,
    };
    use super::super::test_support::key;
    use super::*;
    use crate::config::{AppConfig, RoleSource};
    use crate::paths::JackinPaths;
    use std::collections::BTreeMap;

    fn confirm_modal(
        settings: &mut SettingsState<'_>,
        config: &mut crate::config::AppConfig,
        paths: &crate::paths::JackinPaths,
        key: KeyEvent,
    ) {
        let outcome = handle_settings_confirm_modal(settings, key);
        if matches!(outcome, SettingsModalOutcome::SaveSettings) {
            match crate::console::services::config::save_settings(
                paths,
                crate::console::services::config::SettingsSaveInput {
                    mounts_original: &settings.mounts.original,
                    mounts_pending: &settings.mounts.pending,
                    env_original: &settings.env.original,
                    env_pending: &settings.env.pending,
                    auth_pending: &settings.auth.pending,
                    original_github_env: &settings.auth.original_github_env,
                    github_env: &settings.auth.github_env,
                    trust_pending: &settings.trust.pending,
                    git_coauthor_trailer: settings.general.pending_coauthor_trailer,
                    git_dco: settings.general.pending_dco,
                },
            ) {
                Ok(saved) => {
                    *config = saved;
                    settings.mark_saved();
                    settings.mounts.exit_requested = true;
                }
                Err(err) => settings.mounts.error = Some(err.to_string()),
            }
        }
        if matches!(outcome, SettingsModalOutcome::OpenGlobalMountFileBrowser) {
            match crate::console::services::file_browser::from_home() {
                Ok(file_browser) => {
                    settings
                        .mounts
                        .open_sub_modal(GlobalMountModal::FileBrowser {
                            state: Box::new(file_browser),
                        });
                }
                Err(error) => {
                    settings.mounts.add_draft = None;
                    settings.mounts.error = Some(error.to_string());
                }
            }
        }
        assert!(
            !matches!(outcome, SettingsModalOutcome::OpenUrl(_)),
            "test helper did not expect URL-open"
        );
    }

    #[test]
    fn global_mount_save_detects_sensitive_sources() {
        let rows = vec![crate::config::GlobalMountRow {
            scope: None,
            name: "ssh".into(),
            mount: crate::workspace::MountConfig {
                src: "/home/user/.ssh".into(),
                dst: "/ssh".into(),
                readonly: true,
                isolation: crate::isolation::MountIsolation::Shared,
            },
        }];

        assert!(crate::console::domain::global_rows_have_sensitive_mount(&rows));
    }

    #[test]
    fn add_flow_asks_scope_before_workspace_mount_flow() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let mut config = AppConfig::default();
        let mut state = ManagerState::from_config(&config, tmp.path());
        let mut settings = SettingsState::from_config(&config);
        settings.active_tab = SettingsTab::Mounts;
        state.stage = ManagerStage::Settings(settings);

        handle_settings_key(&mut state, key(KeyCode::Char('a')));
        let ManagerStage::Settings(settings) = &mut state.stage else {
            panic!("expected settings stage");
        };
        assert!(matches!(
            settings.mounts.modal,
            Some(GlobalMountModal::ScopePicker { .. })
        ));

        confirm_modal(settings, &mut config, &paths, key(KeyCode::Enter));
        assert!(matches!(
            settings.mounts.modal,
            Some(GlobalMountModal::FileBrowser { .. })
        ));
    }

    #[test]
    fn global_mount_add_filebrowser_esc_closes_chain() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let mut config = AppConfig::default();
        let mut state = ManagerState::from_config(&config, tmp.path());
        let mut settings = SettingsState::from_config(&config);
        settings.active_tab = SettingsTab::Mounts;
        state.stage = ManagerStage::Settings(settings);

        handle_settings_key(&mut state, key(KeyCode::Char('a')));
        let ManagerStage::Settings(settings) = &mut state.stage else {
            panic!("expected settings stage");
        };
        confirm_modal(settings, &mut config, &paths, key(KeyCode::Enter));
        assert!(matches!(
            settings.mounts.modal,
            Some(GlobalMountModal::FileBrowser { .. })
        ));

        confirm_modal(settings, &mut config, &paths, key(KeyCode::Esc));

        // The ScopePicker was committed when AllAgents was picked, so Esc
        // on the FileBrowser must close the modal chain entirely rather
        // than resurrect a consumed picker.
        assert!(
            settings.mounts.modal.is_none(),
            "Esc from add-mount FileBrowser should close the chain; got {:?}",
            settings.mounts.modal
        );
    }

    #[test]
    fn global_mount_filebrowser_open_git_url_returns_typed_outcome() {
        let tmp = tempfile::tempdir().unwrap();
        let mut settings = SettingsState::from_config(&AppConfig::default());
        let mut browser =
            jackin_console::tui::components::file_browser::FileBrowserState::from_listing(
                jackin_console::services::file_browser::listing_from_home().unwrap(),
            );
        browser.pending_git_prompt = Some(tmp.path().to_path_buf());
        browser.pending_git_url = Some("file:///tmp/settings-url".into());
        settings.mounts.modal = Some(GlobalMountModal::FileBrowser {
            state: Box::new(browser),
        });

        let outcome = handle_settings_confirm_modal(&mut settings, key(KeyCode::Char('O')));

        assert!(matches!(
            outcome,
            SettingsModalOutcome::OpenUrl(url) if url == "file:///tmp/settings-url"
        ));
        assert!(matches!(
            settings.mounts.modal,
            Some(GlobalMountModal::FileBrowser { .. })
        ));
    }

    #[test]
    fn add_flow_specific_scope_uses_shared_role_picker() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let mut config = AppConfig::default();
        config.roles.insert(
            "agent-smith".into(),
            RoleSource {
                git: "https://github.com/jackin-project/jackin-agent-smith.git".into(),
                trusted: true,
                env: BTreeMap::new(),
            },
        );
        let mut state = ManagerState::from_config(&config, tmp.path());
        let mut settings = SettingsState::from_config(&config);
        settings.active_tab = SettingsTab::Mounts;
        state.stage = ManagerStage::Settings(settings);

        handle_settings_key(&mut state, key(KeyCode::Char('a')));
        let ManagerStage::Settings(settings) = &mut state.stage else {
            panic!("expected settings stage");
        };
        let Some(GlobalMountModal::ScopePicker { state: picker }) = settings.mounts.modal.as_mut()
        else {
            panic!("expected scope picker");
        };
        picker.focused = jackin_console::tui::components::scope_picker::ScopeChoice::SpecificAgent;
        confirm_modal(settings, &mut config, &paths, key(KeyCode::Enter));
        assert!(matches!(
            settings.mounts.modal,
            Some(GlobalMountModal::RolePicker { .. })
        ));

        confirm_modal(settings, &mut config, &paths, key(KeyCode::Enter));
        assert!(matches!(
            settings.mounts.modal,
            Some(GlobalMountModal::FileBrowser { .. })
        ));
        assert_eq!(
            settings
                .mounts
                .add_draft
                .as_ref()
                .and_then(|draft| draft.scope.as_deref()),
            Some("agent-smith")
        );
    }

    #[test]
    fn global_mount_role_picker_esc_returns_scope_picker() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let mut config = AppConfig::default();
        config.roles.insert(
            "agent-smith".into(),
            RoleSource {
                git: "https://github.com/jackin-project/jackin-agent-smith.git".into(),
                trusted: true,
                env: BTreeMap::new(),
            },
        );
        let mut state = ManagerState::from_config(&config, tmp.path());
        let mut settings = SettingsState::from_config(&config);
        settings.active_tab = SettingsTab::Mounts;
        state.stage = ManagerStage::Settings(settings);

        handle_settings_key(&mut state, key(KeyCode::Char('a')));
        let ManagerStage::Settings(settings) = &mut state.stage else {
            panic!("expected settings stage");
        };
        let Some(GlobalMountModal::ScopePicker { state: picker }) = settings.mounts.modal.as_mut()
        else {
            panic!("expected scope picker");
        };
        picker.focused = jackin_console::tui::components::scope_picker::ScopeChoice::SpecificAgent;
        confirm_modal(settings, &mut config, &paths, key(KeyCode::Enter));
        assert!(matches!(
            settings.mounts.modal,
            Some(GlobalMountModal::RolePicker { .. })
        ));

        confirm_modal(settings, &mut config, &paths, key(KeyCode::Esc));

        assert!(
            settings.mounts.modal.is_none(),
            "Esc from global-mount RolePicker should close the chain; got {:?}",
            settings.mounts.modal
        );
    }

    #[test]
    fn settings_tab_navigation_reaches_all_config_tabs() {
        // W3C ARIA Tabs: Right cycles tabs when the tab bar has focus.
        let tmp = tempfile::tempdir().unwrap();
        let config = AppConfig::default();
        let mut state = ManagerState::from_config(&config, tmp.path());
        state.stage = ManagerStage::Settings(SettingsState::from_config(&config));
        // Settings opens with tab_bar_focused = true; Right cycles forward.
        assert!(
            matches!(&state.stage, ManagerStage::Settings(s) if s.tab_bar_focused),
            "must start on tab bar"
        );

        // Settings opens on General (first tab); Right cycles: General → Mounts → Environments → Auth → Trust → General
        handle_settings_key(&mut state, key(KeyCode::Right));
        assert!(
            matches!(&state.stage, ManagerStage::Settings(settings) if settings.active_tab == SettingsTab::Mounts)
        );
        handle_settings_key(&mut state, key(KeyCode::Right));
        assert!(
            matches!(&state.stage, ManagerStage::Settings(settings) if settings.active_tab == SettingsTab::Environments)
        );
        handle_settings_key(&mut state, key(KeyCode::Right));
        assert!(
            matches!(&state.stage, ManagerStage::Settings(settings) if settings.active_tab == SettingsTab::Auth)
        );
        handle_settings_key(&mut state, key(KeyCode::Right));
        assert!(
            matches!(&state.stage, ManagerStage::Settings(settings) if settings.active_tab == SettingsTab::Trust)
        );
        handle_settings_key(&mut state, key(KeyCode::Right));
        assert!(
            matches!(&state.stage, ManagerStage::Settings(settings) if settings.active_tab == SettingsTab::General)
        );
    }

    #[test]
    fn settings_tab_bar_follows_aria_focus_pattern() {
        let tmp = tempfile::tempdir().unwrap();
        let config = AppConfig::default();
        let mut state = ManagerState::from_config(&config, tmp.path());
        state.stage = ManagerStage::Settings(SettingsState::from_config(&config));

        handle_settings_key(&mut state, key(KeyCode::Down));
        assert!(
            matches!(&state.stage, ManagerStage::Settings(settings) if !settings.tab_bar_focused),
            "Down from focused tab bar must enter content",
        );

        handle_settings_key(&mut state, key(KeyCode::BackTab));
        assert!(
            matches!(&state.stage, ManagerStage::Settings(settings) if settings.tab_bar_focused),
            "ShiftTab from content must return to tab bar",
        );

        handle_settings_key(&mut state, key(KeyCode::Tab));
        assert!(
            matches!(&state.stage, ManagerStage::Settings(settings) if !settings.tab_bar_focused),
            "Tab from focused tab bar must enter content",
        );

        handle_settings_key(&mut state, key(KeyCode::Esc));
        assert!(
            matches!(&state.stage, ManagerStage::Settings(settings) if settings.tab_bar_focused),
            "Esc from content must return to tab bar",
        );
    }

    #[test]
    fn trust_tab_space_toggles_trusted_state() {
        let tmp = tempfile::tempdir().unwrap();
        let mut config = AppConfig::default();
        config.roles.insert(
            "agent-smith".into(),
            RoleSource {
                git: "https://github.com/jackin-project/jackin-agent-smith.git".into(),
                trusted: true,
                env: BTreeMap::new(),
            },
        );
        let mut state = ManagerState::from_config(&config, tmp.path());
        let mut settings = SettingsState::from_config(&config);
        settings.active_tab = SettingsTab::Trust;
        settings.tab_bar_focused = false;
        state.stage = ManagerStage::Settings(settings);

        let ManagerStage::Settings(settings) = &state.stage else {
            panic!("expected settings stage");
        };
        assert!(settings.trust.pending[0].trusted);

        handle_settings_key(&mut state, key(KeyCode::Char(' ')));
        let ManagerStage::Settings(settings) = &state.stage else {
            panic!("expected settings stage");
        };
        assert!(!settings.trust.pending[0].trusted);

        handle_settings_key(&mut state, key(KeyCode::Char(' ')));
        let ManagerStage::Settings(settings) = &state.stage else {
            panic!("expected settings stage");
        };
        assert!(settings.trust.pending[0].trusted);
    }

    #[test]
    fn general_tab_space_toggles_both_rows() {
        let tmp = tempfile::tempdir().unwrap();
        let config = AppConfig::default();
        let mut state = ManagerState::from_config(&config, tmp.path());
        let mut settings = SettingsState::from_config(&config);
        settings.active_tab = SettingsTab::General;
        settings.tab_bar_focused = false;
        state.stage = ManagerStage::Settings(settings);

        // row 0 (coauthor_trailer) — default is false
        let ManagerStage::Settings(settings) = &state.stage else {
            panic!("expected settings stage");
        };
        assert_eq!(settings.general.selected, 0);
        assert!(!settings.general.pending_coauthor_trailer);

        handle_settings_key(&mut state, key(KeyCode::Char(' ')));
        let ManagerStage::Settings(settings) = &state.stage else {
            panic!("expected settings stage");
        };
        assert!(settings.general.pending_coauthor_trailer);

        handle_settings_key(&mut state, key(KeyCode::Char(' ')));
        let ManagerStage::Settings(settings) = &state.stage else {
            panic!("expected settings stage");
        };
        assert!(!settings.general.pending_coauthor_trailer);

        // navigate to row 1 (dco)
        handle_settings_key(&mut state, key(KeyCode::Down));
        let ManagerStage::Settings(settings) = &state.stage else {
            panic!("expected settings stage");
        };
        assert_eq!(settings.general.selected, 1);
        assert!(!settings.general.pending_dco);

        handle_settings_key(&mut state, key(KeyCode::Char(' ')));
        let ManagerStage::Settings(settings) = &state.stage else {
            panic!("expected settings stage");
        };
        assert!(settings.general.pending_dco);

        handle_settings_key(&mut state, key(KeyCode::Char(' ')));
        let ManagerStage::Settings(settings) = &state.stage else {
            panic!("expected settings stage");
        };
        assert!(!settings.general.pending_dco);

        // navigate back to row 0
        handle_settings_key(&mut state, key(KeyCode::Up));
        let ManagerStage::Settings(settings) = &state.stage else {
            panic!("expected settings stage");
        };
        assert_eq!(settings.general.selected, 0);
    }

    #[test]
    fn general_tab_enter_does_not_toggle_rows() {
        for selected in [0usize, 1usize] {
            let tmp = tempfile::tempdir().unwrap();
            let config = AppConfig::default();
            let mut state = ManagerState::from_config(&config, tmp.path());
            let mut settings = SettingsState::from_config(&config);
            settings.active_tab = SettingsTab::General;
            settings.tab_bar_focused = false;
            settings.general.selected = selected;
            state.stage = ManagerStage::Settings(settings);

            handle_settings_key(&mut state, key(KeyCode::Enter));

            let ManagerStage::Settings(settings) = &state.stage else {
                panic!("expected settings stage");
            };
            assert!(
                !settings.general.pending_coauthor_trailer,
                "Enter on settings General row {selected} must not toggle co-author trailer",
            );
            assert!(
                !settings.general.pending_dco,
                "Enter on settings General row {selected} must not toggle DCO",
            );
        }
    }

    #[test]
    fn trust_tab_enter_does_not_toggle_trusted_state() {
        let tmp = tempfile::tempdir().unwrap();
        let mut config = AppConfig::default();
        config.roles.insert(
            "agent-smith".into(),
            RoleSource {
                git: "https://github.com/jackin-project/jackin-agent-smith.git".into(),
                trusted: true,
                env: BTreeMap::new(),
            },
        );
        let mut state = ManagerState::from_config(&config, tmp.path());
        let mut settings = SettingsState::from_config(&config);
        settings.active_tab = SettingsTab::Trust;
        settings.tab_bar_focused = false;
        state.stage = ManagerStage::Settings(settings);

        handle_settings_key(&mut state, key(KeyCode::Enter));

        let ManagerStage::Settings(settings) = &state.stage else {
            panic!("expected settings stage");
        };
        assert!(
            settings.trust.pending[0].trusted,
            "Enter on Trust row must not toggle trusted state",
        );
    }

    #[test]
    fn auth_tab_mode_row_ignores_space_and_enter_opens_form() {
        let tmp = tempfile::tempdir().unwrap();
        let config = AppConfig::default();
        let mut state = ManagerState::from_config(&config, tmp.path());
        let mut settings = SettingsState::from_config(&config);
        settings.active_tab = SettingsTab::Auth;
        settings.tab_bar_focused = false;
        state.stage = ManagerStage::Settings(settings);

        handle_settings_key(&mut state, key(KeyCode::Enter));
        handle_settings_key(&mut state, key(KeyCode::Char(' ')));

        let ManagerStage::Settings(settings) = &state.stage else {
            panic!("expected settings stage");
        };
        assert_eq!(
            settings.auth.pending[0].mode,
            jackin_console::tui::auth::AuthMode::Sync
        );
        assert!(!settings.auth.is_dirty());
        assert!(settings.auth.modal.is_none());

        handle_settings_key(&mut state, key(KeyCode::Enter));

        let ManagerStage::Settings(settings) = &state.stage else {
            panic!("expected settings stage");
        };
        assert!(matches!(
            settings.auth.modal,
            Some(SettingsAuthModal::AuthForm { .. })
        ));
    }

    /// `g` on the global Claude `oauth_token` auth form opens the
    /// shared source picker (plain vs. 1Password) and arms
    /// `generating_token`, driving the global token-generate (mint)
    /// path. The storage-target choice happens at the source picker.
    #[test]
    fn settings_auth_generate_opens_source_picker_and_arms_flag() {
        use jackin_console::tui::auth::{AuthKind, AuthMode};

        let config = AppConfig::default();
        let mut settings = SettingsState::from_config(&config);
        settings.active_tab = SettingsTab::Auth;
        settings.tab_bar_focused = false;
        settings.auth.selected_kind = Some(AuthKind::Claude);
        open_settings_auth_form(&mut settings.auth, &settings.env);
        // Drive the mode to OAuthToken so the generate gate holds.
        let Some(SettingsAuthModal::AuthForm { state: form, .. }) = settings.auth.modal.as_mut()
        else {
            panic!("auth form must be open");
        };
        form.set_mode(AuthMode::OAuthToken);
        assert!(settings_auth_can_generate_token(&settings.auth));

        let op_cache = std::rc::Rc::new(std::cell::RefCell::new(
            crate::operator_env::OpCache::default(),
        ));
        let mut pending = None;
        handle_settings_auth_modal(
            &mut settings.auth,
            &mut settings.env,
            &mut pending,
            key(KeyCode::Char('g')),
            true,
            op_cache,
        );

        assert!(
            matches!(
                settings.auth.modal,
                Some(SettingsAuthModal::SourcePicker { .. })
            ),
            "generate must open the source picker as the first step"
        );
        assert!(
            !settings.auth.modal_parents.is_empty(),
            "generate must stash the form so the post-mint re-mount can return to it; \
             generate vs. provide is disambiguated by the generate flag, not the stash"
        );
        assert!(
            settings.auth.generating_token,
            "generate must arm the global token-generate flag"
        );
        assert!(
            pending.is_none(),
            "no mint request is built until the source/picker commits"
        );
    }

    /// After the settings `g`/`G` generate stashes the form, the mint
    /// completion re-mounts the global Claude Edit-auth dialog with the
    /// minted op credential applied and focus on Save — the shape the
    /// `run_console` loop drives via `apply_op_picker_to_settings_auth_form`.
    /// Nothing is persisted here; the operator's Save does that. Uses an
    /// injected stub `OpRunner` so no real `op` binary runs.
    #[test]
    fn settings_auth_generate_op_mint_remounts_form_focus_save() {
        use jackin_console::tui::auth::{AuthKind, AuthMode};
        use crate::operator_env::{OpRef, OpRunner};

        struct StubRunner;
        impl OpRunner for StubRunner {
            fn read(&self, _r: &str) -> anyhow::Result<String> {
                Ok("sk-ant-oat01-MINTED".into())
            }
        }

        let config = AppConfig::default();
        let mut settings = SettingsState::from_config(&config);
        settings.active_tab = SettingsTab::Auth;
        settings.tab_bar_focused = false;
        settings.auth.selected_kind = Some(AuthKind::Claude);
        open_settings_auth_form(&mut settings.auth, &settings.env);
        let Some(SettingsAuthModal::AuthForm { state: form, .. }) = settings.auth.modal.as_mut()
        else {
            panic!("auth form must be open");
        };
        form.set_mode(AuthMode::OAuthToken);

        // Press `g` to start generate (stashes the form).
        let op_cache = std::rc::Rc::new(std::cell::RefCell::new(
            crate::operator_env::OpCache::default(),
        ));
        let mut pending = None;
        handle_settings_auth_modal(
            &mut settings.auth,
            &mut settings.env,
            &mut pending,
            key(KeyCode::Char('g')),
            true,
            op_cache,
        );
        assert!(!settings.auth.modal_parents.is_empty());

        // Simulate the loop's post-mint re-mount with the wired OpRef.
        let minted = OpRef {
            op: "op://uuid/claude-vault".into(),
            path: "Personal/Claude/oauth-token".into(),
            account: None,
        };
        apply_op_picker_to_settings_auth_form_with_runner(
            &mut settings.auth,
            minted.clone(),
            &StubRunner,
        );

        let Some(SettingsAuthModal::AuthForm { state, focus, .. }) = &settings.auth.modal else {
            panic!("mint completion must re-mount the settings auth form");
        };
        assert_eq!(
            *focus,
            AuthFormFocus::Save,
            "post-mint re-mount drops the cursor onto Save"
        );
        match &state.credential {
            CredentialInput::OpRef(r) => assert_eq!(r, &minted),
            other => panic!("expected OpRef credential after mint; got {other:?}"),
        }
        assert!(settings.auth.modal_parents.is_empty());
        assert!(
            pending.is_none(),
            "the mint request was already drained by the loop; none re-queued"
        );
    }

    /// `g` is a no-op on the global Claude form when the mode is not
    /// `oauth_token` (here ApiKey): the auth form stays open and the
    /// generate flag is not armed.
    #[test]
    fn settings_auth_generate_is_noop_for_non_oauth_token_mode() {
        use jackin_console::tui::auth::{AuthKind, AuthMode};

        let config = AppConfig::default();
        let mut settings = SettingsState::from_config(&config);
        settings.active_tab = SettingsTab::Auth;
        settings.tab_bar_focused = false;
        settings.auth.selected_kind = Some(AuthKind::Claude);
        open_settings_auth_form(&mut settings.auth, &settings.env);
        let Some(SettingsAuthModal::AuthForm { state: form, .. }) = settings.auth.modal.as_mut()
        else {
            panic!("auth form must be open");
        };
        form.set_mode(AuthMode::ApiKey);
        assert!(!settings_auth_can_generate_token(&settings.auth));

        let op_cache = std::rc::Rc::new(std::cell::RefCell::new(
            crate::operator_env::OpCache::default(),
        ));
        let mut pending = None;
        handle_settings_auth_modal(
            &mut settings.auth,
            &mut settings.env,
            &mut pending,
            key(KeyCode::Char('g')),
            true,
            op_cache,
        );

        assert!(matches!(
            settings.auth.modal,
            Some(SettingsAuthModal::AuthForm { .. })
        ));
        assert!(!settings.auth.generating_token);
        assert!(pending.is_none());
    }

    #[test]
    fn env_tab_add_flow_asks_scope_before_key() {
        let tmp = tempfile::tempdir().unwrap();
        let config = AppConfig::default();
        let mut state = ManagerState::from_config(&config, tmp.path());
        let mut settings = SettingsState::from_config(&config);
        settings.active_tab = SettingsTab::Environments;
        settings.tab_bar_focused = false;
        state.stage = ManagerStage::Settings(settings);

        handle_settings_key(&mut state, key(KeyCode::Enter));
        let ManagerStage::Settings(settings) = &mut state.stage else {
            panic!("expected settings stage");
        };
        assert!(matches!(
            settings.env.modal,
            Some(SettingsEnvModal::ScopePicker { .. })
        ));

        handle_settings_env_modal(
            &mut settings.env,
            key(KeyCode::Enter),
            state.op_cache.clone(),
        );
        assert!(matches!(
            settings.env.modal,
            Some(SettingsEnvModal::Text {
                target: SettingsEnvTextTarget::EnvKey {
                    scope: crate::console::tui::state::SettingsEnvScope::Global
                },
                ..
            })
        ));
    }

    #[test]
    fn env_tab_key_input_esc_closes_chain() {
        let tmp = tempfile::tempdir().unwrap();
        let config = AppConfig::default();
        let mut state = ManagerState::from_config(&config, tmp.path());
        let mut settings = SettingsState::from_config(&config);
        settings.active_tab = SettingsTab::Environments;
        settings.tab_bar_focused = false;
        state.stage = ManagerStage::Settings(settings);

        handle_settings_key(&mut state, key(KeyCode::Enter));
        let ManagerStage::Settings(settings) = &mut state.stage else {
            panic!("expected settings stage");
        };
        handle_settings_env_modal(
            &mut settings.env,
            key(KeyCode::Enter),
            state.op_cache.clone(),
        );
        assert!(matches!(
            settings.env.modal,
            Some(SettingsEnvModal::Text {
                target: SettingsEnvTextTarget::EnvKey { .. },
                ..
            })
        ));

        handle_settings_env_modal(&mut settings.env, key(KeyCode::Esc), state.op_cache.clone());

        // The ScopePicker was committed before the EnvKey input opened,
        // so Esc on the input must close the chain instead of restoring
        // a consumed picker.
        assert!(
            settings.env.modal.is_none(),
            "Esc from settings env key input should close the chain; got {:?}",
            settings.env.modal
        );
    }

    #[test]
    fn env_tab_source_picker_esc_returns_key_input() {
        let tmp = tempfile::tempdir().unwrap();
        let config = AppConfig::default();
        let mut state = ManagerState::from_config(&config, tmp.path());
        let mut settings = SettingsState::from_config(&config);
        settings.active_tab = SettingsTab::Environments;
        settings.tab_bar_focused = false;
        state.stage = ManagerStage::Settings(settings);

        handle_settings_key(&mut state, key(KeyCode::Enter));
        let ManagerStage::Settings(settings) = &mut state.stage else {
            panic!("expected settings stage");
        };
        handle_settings_env_modal(
            &mut settings.env,
            key(KeyCode::Enter),
            state.op_cache.clone(),
        );
        let target = SettingsEnvTextTarget::EnvKey {
            scope: crate::console::tui::state::SettingsEnvScope::Global,
        };
        commit_env_text(&mut settings.env, &target, "API_KEY");
        assert!(matches!(
            settings.env.modal,
            Some(SettingsEnvModal::SourcePicker { .. })
        ));

        handle_settings_env_modal(&mut settings.env, key(KeyCode::Esc), state.op_cache.clone());

        assert!(
            matches!(
                settings.env.modal,
                Some(SettingsEnvModal::Text {
                    target: SettingsEnvTextTarget::EnvKey { .. },
                    ..
                })
            ),
            "Esc from settings env SourcePicker should restore key input; got {:?}",
            settings.env.modal
        );
    }

    #[test]
    fn env_tab_specific_scope_uses_workspace_role_picker() {
        let tmp = tempfile::tempdir().unwrap();
        let mut config = AppConfig::default();
        config.roles.insert(
            "chainargos/agent-brown".into(),
            RoleSource {
                git: "https://example.invalid/brown.git".into(),
                trusted: false,
                env: BTreeMap::new(),
            },
        );
        let mut state = ManagerState::from_config(&config, tmp.path());
        let mut settings = SettingsState::from_config(&config);
        settings.active_tab = SettingsTab::Environments;
        settings.tab_bar_focused = false;
        state.stage = ManagerStage::Settings(settings);

        handle_settings_key(&mut state, key(KeyCode::Enter));
        let ManagerStage::Settings(settings) = &mut state.stage else {
            panic!("expected settings stage");
        };
        let Some(SettingsEnvModal::ScopePicker { state: picker }) = settings.env.modal.as_mut()
        else {
            panic!("expected scope picker");
        };
        picker.focused = jackin_console::tui::components::scope_picker::ScopeChoice::SpecificAgent;
        handle_settings_env_modal(
            &mut settings.env,
            key(KeyCode::Enter),
            state.op_cache.clone(),
        );
        assert!(matches!(
            settings.env.modal,
            Some(SettingsEnvModal::RolePicker { .. })
        ));

        handle_settings_env_modal(
            &mut settings.env,
            key(KeyCode::Enter),
            state.op_cache.clone(),
        );
        assert!(matches!(
            &settings.env.modal,
            Some(SettingsEnvModal::Text {
                target: SettingsEnvTextTarget::EnvKey {
                    scope: crate::console::tui::state::SettingsEnvScope::Role(role)
                },
                ..
            }) if role == "chainargos/agent-brown"
        ));
    }

    #[test]
    fn settings_env_rows_hide_roles_without_env_entries() {
        let mut config = AppConfig::default();
        config.roles.insert(
            "agent-empty".into(),
            RoleSource {
                git: "https://example.invalid/empty.git".into(),
                trusted: false,
                env: BTreeMap::new(),
            },
        );
        config.roles.insert(
            "agent-with-env".into(),
            RoleSource {
                git: "https://example.invalid/with-env.git".into(),
                trusted: false,
                env: BTreeMap::from([(
                    "ROLE_ALPHA".into(),
                    crate::operator_env::EnvValue::Plain("one".into()),
                )]),
            },
        );
        let settings = SettingsState::from_config(&config);
        let rows = settings_env_flat_rows(&settings);

        assert!(
            !rows
                .iter()
                .any(|row| matches!(row, SettingsEnvRow::RoleHeader { role, .. } if role == "agent-empty")),
            "empty role env sections should stay hidden: {rows:?}"
        );
        assert!(
            rows.iter()
                .any(|row| matches!(row, SettingsEnvRow::RoleHeader { role, .. } if role == "agent-with-env")),
            "roles with env entries should remain visible: {rows:?}"
        );
    }

    #[test]
    fn after_settings_event_promotes_subtab_errors_to_error_popup() {
        fn set_mounts_error(settings: &mut SettingsState<'_>) {
            settings.mounts.error = Some("mounts detail".into());
        }
        fn set_env_error(settings: &mut SettingsState<'_>) {
            settings.env.error = Some("env detail".into());
        }
        fn set_auth_error(settings: &mut SettingsState<'_>) {
            settings.auth.error = Some("auth detail".into());
        }
        fn set_trust_error(settings: &mut SettingsState<'_>) {
            settings.trust.error = Some("trust detail".into());
        }

        for (name, set_error) in [
            ("mounts", set_mounts_error as fn(&mut SettingsState<'_>)),
            ("env", set_env_error),
            ("auth", set_auth_error),
            ("trust", set_trust_error),
        ] {
            let tmp = tempfile::tempdir().unwrap();
            let paths = JackinPaths::for_tests(tmp.path());
            paths.ensure_base_dirs().unwrap();
            let config = AppConfig::default();
            let mut state = ManagerState::from_config(&config, tmp.path());
            let mut settings = SettingsState::from_config(&config);
            set_error(&mut settings);
            state.stage = ManagerStage::Settings(settings);

            after_settings_event(&mut state);

            let ManagerStage::Settings(settings) = &state.stage else {
                panic!("must stay in Settings stage");
            };
            let popup = settings
                .error_popup
                .as_ref()
                .unwrap_or_else(|| panic!("{name} error must promote to ErrorPopup"));
            assert_eq!(popup.title, "Settings error");
            assert!(
                popup.message.contains(name),
                "{name} error detail must survive promotion: {:?}",
                popup.message,
            );
            assert!(settings.mounts.error.is_none());
            assert!(settings.env.error.is_none());
            assert!(settings.auth.error.is_none());
            assert!(settings.trust.error.is_none());
        }
    }

    #[test]
    fn after_settings_event_exit_requested_pops_to_list() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let config = AppConfig::default();
        let mut state = ManagerState::from_config(&config, tmp.path());
        let mut settings = SettingsState::from_config(&config);
        settings.mounts.exit_requested = true;
        state.stage = ManagerStage::Settings(settings);

        after_settings_event(&mut state);

        assert!(
            matches!(state.stage, ManagerStage::List),
            "exit_requested must pop to List; got {:?}",
            state.stage,
        );
    }
}
