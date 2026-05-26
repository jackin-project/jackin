use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::super::render::apply_scroll_delta;
use super::super::render::global_mounts::{SettingsEnvRow, settings_env_flat_rows};
use super::super::state::{
    AuthFormFocus, AuthFormReturnPath, AuthFormTarget, GlobalMountConfirm, GlobalMountDraft,
    GlobalMountModal, GlobalMountTextTarget, ManagerStage, ManagerState, SettingsAuthModal,
    SettingsEnvConfirm, SettingsEnvModal, SettingsEnvScope, SettingsEnvTextTarget, SettingsTab,
};
use crate::config::AppConfig;
use crate::console::widgets::ModalOutcome;
use crate::console::widgets::auth_panel::{AuthForm, CredentialInput};
use crate::console::widgets::confirm::ConfirmState;
use crate::console::widgets::file_browser::FileBrowserState;
use crate::console::widgets::role_picker::RolePickerState;
use crate::console::widgets::text_input::TextInputState;
use crate::paths::JackinPaths;
use crate::selector::RoleSelector;
use crate::workspace::{MountConfig, resolve_path};

const MOUNT_NAME_EMPTY: &str = "Mount name cannot be empty.";
const MOUNT_GONE: &str = "Mount no longer exists; selection was cleared.";
const ADD_DRAFT_LOST: &str = "Add-mount draft was lost; press 'a' to start over.";

pub(super) fn handle_settings_key(state: &mut ManagerState<'_>, key: KeyEvent) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };

    // W3C ARIA Tabs: when tab_bar_focused, Left/Right cycle tabs and Tab/↓
    // enters the content area.
    if settings.tab_bar_focused {
        match key.code {
            KeyCode::Left | KeyCode::BackTab => {
                settings.active_tab = settings.active_tab.previous();
                return;
            }
            KeyCode::Right => {
                settings.active_tab = settings.active_tab.next();
                return;
            }
            KeyCode::Tab | KeyCode::Down | KeyCode::Char('j') => {
                settings.tab_bar_focused = false;
                return;
            }
            _ => {}
        }
        // All other keys (S, Esc, etc.) fall through to content handling.
    }

    match key.code {
        // Right on an Environments role header expands it; Right elsewhere is
        // intra-area and must not cycle tabs.
        KeyCode::Right if settings.active_tab == SettingsTab::Environments => {
            let rows = settings_env_flat_rows(settings);
            if let Some(SettingsEnvRow::RoleHeader { role, expanded }) =
                rows.get(settings.env.selected).cloned()
                && !expanded
            {
                settings.env.expanded.insert(role);
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
                settings.env.expanded.remove(&role);
            }
            return;
        }
        // Tab from content returns to tab bar and advances to next tab.
        KeyCode::Tab => {
            settings.tab_bar_focused = true;
            settings.active_tab = settings.active_tab.next();
            return;
        }
        // BackTab from content returns focus to tab bar without changing tab.
        KeyCode::BackTab => {
            settings.tab_bar_focused = true;
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

fn handle_global_mounts_key(state: &mut ManagerState<'_>, key: KeyEvent) {
    // S is handled here, before `global` borrows `settings.mounts`, so
    // `open_settings_save_preview` can receive all of `settings`.
    if matches!(key.code, KeyCode::Char('s' | 'S')) {
        let ManagerStage::Settings(settings) = &mut state.stage else {
            return;
        };
        if has_sensitive_mount(&settings.mounts.pending) {
            settings.mounts.modal = Some(confirm_modal(GlobalMountConfirm::Sensitive));
        } else {
            open_settings_save_preview(settings);
        }
        return;
    }

    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    let is_dirty = settings.is_dirty();
    let global = &mut settings.mounts;
    match key.code {
        KeyCode::Esc | KeyCode::Char('q' | 'Q') => {
            if is_dirty {
                global.modal = Some(confirm_modal(GlobalMountConfirm::Discard));
            } else {
                state.stage = ManagerStage::List;
            }
        }
        KeyCode::Char('h' | 'H') => {
            apply_scroll_delta(&mut global.scroll_x, -8);
        }
        KeyCode::Char('l' | 'L') => {
            apply_scroll_delta(&mut global.scroll_x, 8);
        }
        KeyCode::Up | KeyCode::Char('k' | 'K') => {
            global.selected = global.selected.saturating_sub(1);
            global.scroll_y = super::super::render::cursor_scroll_for_panel(
                global.selected,
                global.scroll_y,
                state.cached_term_size,
            );
        }
        KeyCode::Down | KeyCode::Char('j' | 'J') => {
            let max = global.pending.len();
            global.selected = (global.selected + 1).min(max);
            global.scroll_y = super::super::render::cursor_scroll_for_panel(
                global.selected,
                global.scroll_y,
                state.cached_term_size,
            );
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
        KeyCode::Char('r' | 'R') => {
            if let Some(row) = global.pending.get_mut(global.selected) {
                row.mount.readonly = !row.mount.readonly;
            }
        }
        KeyCode::Char('o' | 'O') => {
            if let Some(row) = global.pending.get(global.selected) {
                match super::super::mount_info::inspect(&row.mount.src) {
                    super::super::mount_info::MountKind::Git {
                        origin: Some(super::super::mount_info::GitOrigin::Github { web_url, .. }),
                        ..
                    } => {
                        if let Err(err) = open::that_detached(&web_url) {
                            global.error = Some(format!("failed to open URL: {err}"));
                        }
                    }
                    super::super::mount_info::MountKind::Git { .. }
                    | super::super::mount_info::MountKind::Folder
                    | super::super::mount_info::MountKind::Missing => {
                        global.error = Some("no GitHub URL for this mount".into());
                    }
                }
            }
        }
        KeyCode::Char('n' | 'N') => open_edit_text(state, GlobalMountTextTarget::Rename),
        KeyCode::Char('1') => open_edit_text(state, GlobalMountTextTarget::Source),
        KeyCode::Char('2') => open_edit_text(state, GlobalMountTextTarget::Destination),
        KeyCode::Char('3') => open_edit_text(state, GlobalMountTextTarget::Scope),
        _ => {}
    }
}

fn handle_env_key(state: &mut ManagerState<'_>, key: KeyEvent) {
    let op_cache = state.op_cache.clone();
    let op_available = state.op_available;
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    match key.code {
        KeyCode::Esc | KeyCode::Char('q' | 'Q') => {
            if settings.is_dirty() {
                settings.mounts.modal = Some(confirm_modal(GlobalMountConfirm::Discard));
            } else {
                state.stage = ManagerStage::List;
            }
        }
        KeyCode::Up | KeyCode::Char('k' | 'K') => {
            let rows = settings_env_flat_rows(settings);
            settings.env.selected =
                step_settings_env_cursor_up(&rows, settings.env.selected.saturating_sub(1));
            settings.env.scroll_y = super::super::render::cursor_scroll_for_panel(
                settings.env.selected,
                settings.env.scroll_y,
                state.cached_term_size,
            );
        }
        KeyCode::Down | KeyCode::Char('j' | 'J') => {
            let rows = settings_env_flat_rows(settings);
            let max = rows.len().saturating_sub(1);
            let candidate = (settings.env.selected + 1).min(max);
            settings.env.selected = step_settings_env_cursor_down(&rows, candidate, max);
            settings.env.scroll_y = super::super::render::cursor_scroll_for_panel(
                settings.env.selected,
                settings.env.scroll_y,
                state.cached_term_size,
            );
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
                    if settings_env_value(&settings.env, scope, key)
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
}

fn step_settings_env_cursor_down(rows: &[SettingsEnvRow], candidate: usize, max: usize) -> usize {
    let mut idx = candidate;
    while idx <= max {
        match rows.get(idx) {
            Some(SettingsEnvRow::SectionSpacer) => idx += 1,
            _ => return idx,
        }
    }
    candidate
}

fn step_settings_env_cursor_up(rows: &[SettingsEnvRow], candidate: usize) -> usize {
    let mut idx = candidate;
    loop {
        match rows.get(idx) {
            Some(SettingsEnvRow::SectionSpacer) => {
                if idx == 0 {
                    return 0;
                }
                idx -= 1;
            }
            _ => return idx,
        }
    }
}

fn handle_auth_key(state: &mut ManagerState<'_>, key: KeyEvent) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    match key.code {
        KeyCode::Esc | KeyCode::Char('q' | 'Q') => {
            if settings.auth.selected_kind.is_some() {
                settings.auth.selected_kind = None;
                settings.auth.selected = 0;
            } else if settings.is_dirty() {
                settings.mounts.modal = Some(confirm_modal(GlobalMountConfirm::Discard));
            } else {
                state.stage = ManagerStage::List;
            }
        }
        KeyCode::Up | KeyCode::Char('k' | 'K') => {
            settings.auth.selected = settings.auth.selected.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j' | 'J') => {
            let max = settings_auth_row_count(&settings.auth).saturating_sub(1);
            settings.auth.selected = (settings.auth.selected + 1).min(max);
        }
        KeyCode::Enter => {
            if settings.auth.selected_kind.is_none() {
                if let Some(row) = settings.auth.pending.get(settings.auth.selected) {
                    settings.auth.selected_kind = Some(row.kind);
                    settings.auth.selected = 0;
                }
            } else {
                open_settings_auth_form(&mut settings.auth, &settings.env);
            }
        }
        KeyCode::Char('s' | 'S') => {
            open_settings_save_preview(settings);
        }
        _ => {}
    }
}

fn settings_auth_row_count(auth: &super::super::state::SettingsAuthState) -> usize {
    let Some(kind) = auth.selected_kind else {
        return auth.pending.len();
    };
    let Some(row) = auth.pending.iter().find(|row| row.kind == kind) else {
        return 0;
    };
    if kind.required_env_var(row.mode).is_some() {
        2
    } else {
        1
    }
}

fn open_settings_auth_form(
    auth: &mut super::super::state::SettingsAuthState,
    env: &super::super::state::SettingsEnvState<'_>,
) {
    let Some(kind) = auth.selected_kind else {
        return;
    };
    let Some(row) = auth.pending.iter().find(|row| row.kind == kind) else {
        return;
    };
    let existing_credential = kind
        .required_env_var(row.mode)
        .and_then(|name| match kind {
            crate::console::manager::auth_kind::AuthKind::Github => auth.github_env.get(name),
            crate::console::manager::auth_kind::AuthKind::Claude
            | crate::console::manager::auth_kind::AuthKind::Codex
            | crate::console::manager::auth_kind::AuthKind::Amp
            | crate::console::manager::auth_kind::AuthKind::Kimi
            | crate::console::manager::auth_kind::AuthKind::Opencode => env.pending.env.get(name),
        })
        .cloned();
    let form = AuthForm::from_existing(kind, row.mode, existing_credential);
    let literal_buffer = if let CredentialInput::Literal(s) = &form.credential {
        s.clone()
    } else {
        String::new()
    };
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
pub(in crate::console::manager) fn settings_auth_can_generate_token(
    auth: &super::super::state::SettingsAuthState,
) -> bool {
    matches!(
        auth.modal.as_ref(),
        Some(SettingsAuthModal::AuthForm { state, .. })
            if state.kind == crate::console::manager::auth_kind::AuthKind::Claude
                && state.mode == Some(crate::console::manager::auth_kind::AuthMode::OAuthToken)
    )
}

#[allow(clippy::too_many_lines)]
pub(super) fn handle_settings_auth_modal(
    auth: &mut super::super::state::SettingsAuthState,
    env: &mut super::super::state::SettingsEnvState<'_>,
    pending_token_generate: &mut Option<super::super::state::PendingTokenGenerate>,
    key: KeyEvent,
    op_available: bool,
    op_cache: std::rc::Rc<std::cell::RefCell<crate::console::op_cache::OpCache>>,
) {
    let Some(mut modal) = auth.modal.take() else {
        return;
    };
    match &mut modal {
        SettingsAuthModal::AuthForm {
            target,
            state,
            focus,
            literal_buffer,
        } => {
            if key.code == KeyCode::Esc {
                auth.pending_auth_form_return = None;
                return;
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
                && state.kind == crate::console::manager::auth_kind::AuthKind::Claude
                && state.mode == Some(crate::console::manager::auth_kind::AuthMode::OAuthToken)
            {
                auth.generating_token = true;
                auth.pending_auth_form_return = Some(AuthFormReturnPath {
                    target: target.clone(),
                    state: std::mem::replace(state, Box::new(AuthForm::new(state.kind))),
                    focus: *focus,
                    literal_buffer: literal_buffer.clone(),
                });
                auth.modal = Some(SettingsAuthModal::SourcePicker {
                    state: crate::console::widgets::source_picker::SourcePickerState::new(
                        "generated token".to_string(),
                        op_available,
                    ),
                });
                return;
            }
            match *focus {
                AuthFormFocus::Mode => match key.code {
                    KeyCode::Char(' ') => cycle_auth_form_mode(state),
                    // Down/j moves within the field area; Tab crosses into the button area.
                    // No credential row: Down is a no-op at the bottom of the field area.
                    KeyCode::Down | KeyCode::Char('j') if state.shows_credential_block() => {
                        *focus = AuthFormFocus::CredentialSource;
                    }
                    KeyCode::Tab => {
                        *focus = if state.shows_credential_block() {
                            AuthFormFocus::CredentialSource
                        } else {
                            AuthFormFocus::Save
                        };
                    }
                    KeyCode::BackTab => *focus = AuthFormFocus::Reset,
                    _ => {}
                },
                AuthFormFocus::CredentialSource => match key.code {
                    KeyCode::Enter => {
                        let Some(env_var) = state.mode.and_then(|m| state.kind.required_env_var(m))
                        else {
                            auth.modal = Some(modal);
                            return;
                        };
                        auth.pending_auth_form_return = Some(AuthFormReturnPath {
                            target: target.clone(),
                            state: std::mem::replace(state, Box::new(AuthForm::new(state.kind))),
                            focus: *focus,
                            literal_buffer: literal_buffer.clone(),
                        });
                        auth.modal = Some(SettingsAuthModal::SourcePicker {
                            state: crate::console::widgets::source_picker::SourcePickerState::new(
                                env_var.to_string(),
                                op_available,
                            ),
                        });
                        return;
                    }
                    // Down/j is a no-op at the bottom of the field area; Tab crosses to button area.
                    KeyCode::Tab => {
                        *focus = AuthFormFocus::Save;
                    }
                    KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => {
                        *focus = AuthFormFocus::Mode;
                    }
                    _ => {}
                },
                AuthFormFocus::Save => match key.code {
                    KeyCode::Right | KeyCode::Tab => *focus = AuthFormFocus::Cancel,
                    // Up is a no-op at the top of the button area; BackTab crosses back to field area.
                    KeyCode::BackTab => {
                        *focus = if state.shows_credential_block() {
                            AuthFormFocus::CredentialSource
                        } else {
                            AuthFormFocus::Mode
                        };
                    }
                    KeyCode::Enter if state.can_save() => {
                        persist_settings_auth_form(auth, env, state);
                        return;
                    }
                    _ => {}
                },
                AuthFormFocus::Cancel => match key.code {
                    KeyCode::Left | KeyCode::BackTab => *focus = AuthFormFocus::Save,
                    KeyCode::Right | KeyCode::Tab => *focus = AuthFormFocus::Reset,
                    KeyCode::Enter => return,
                    _ => {}
                },
                AuthFormFocus::Reset => match key.code {
                    KeyCode::Left | KeyCode::BackTab => *focus = AuthFormFocus::Cancel,
                    // Tab wraps the cycle back to the first field; Right stays on the button row.
                    KeyCode::Tab => *focus = AuthFormFocus::Mode,
                    KeyCode::Enter => {
                        clear_settings_auth_kind(auth, env, target);
                        return;
                    }
                    _ => {}
                },
            }
            auth.modal = Some(modal);
        }
        SettingsAuthModal::SourcePicker { state } => {
            use crate::console::widgets::source_picker::SourceChoice;
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
                        *pending_token_generate = Some(super::super::state::PendingTokenGenerate {
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
                                crate::console::widgets::op_picker::OpPickerState::new_create_with_cache(
                                    op_cache,
                                    crate::workspace::token_setup::DEFAULT_ITEM_TEMPLATE
                                        .replace("{ws}", "global"),
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
                return;
            }
            match outcome {
                ModalOutcome::Commit(SourceChoice::Plain) => {
                    let literal = auth
                        .pending_auth_form_return
                        .as_ref()
                        .map(|return_path| return_path.literal_buffer.clone())
                        .unwrap_or_default();
                    auth.modal = Some(SettingsAuthModal::TextInput {
                        state: Box::new(TextInputState::new("Credential", literal)),
                    });
                }
                ModalOutcome::Commit(SourceChoice::Op) => {
                    auth.modal = Some(SettingsAuthModal::OpPicker {
                        state: Box::new(
                            crate::console::widgets::op_picker::OpPickerState::new_with_cache(
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
                return;
            }
            match outcome {
                // Browse-mode caller: only `Existing` is reachable.
                ModalOutcome::Commit(
                    crate::console::widgets::op_picker::OpPickerSelection::NewItem { .. }
                    | crate::console::widgets::op_picker::OpPickerSelection::EditItemField { .. },
                ) => unreachable!("settings-auth browse OpPicker runs in Browse mode"),
                ModalOutcome::Commit(
                    crate::console::widgets::op_picker::OpPickerSelection::Existing(op_ref),
                ) => apply_op_picker_to_settings_auth_form(auth, op_ref),
                ModalOutcome::Cancel => restore_settings_auth_form(auth),
                ModalOutcome::Continue => auth.modal = Some(modal),
            }
        }
    }
}

/// Translate a Create-mode `OpPicker` commit into a global
/// [`PendingTokenGenerate`](super::super::state::PendingTokenGenerate)
/// request that the `run_console` loop drains to mint the token.
/// `Existing` cannot occur in Create mode; a Cancel (or stray
/// `Existing`) just closes the chain. On `Continue` the picker is still
/// drilling, so the marker stays armed and the modal stays open.
fn handle_settings_token_generate_pick(
    auth: &mut super::super::state::SettingsAuthState,
    pending_token_generate: &mut Option<super::super::state::PendingTokenGenerate>,
    outcome: ModalOutcome<crate::console::widgets::op_picker::OpPickerSelection>,
    modal: SettingsAuthModal<'static>,
) {
    use crate::console::widgets::op_picker::OpPickerSelection;
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
            field_label,
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
                field_label,
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
    *pending_token_generate = Some(super::super::state::PendingTokenGenerate {
        scope: crate::workspace::token_setup::TokenSetupScope::Global,
        args,
    });
}

fn cycle_auth_form_mode(form: &mut AuthForm) {
    let modes = form.available_modes();
    if modes.is_empty() {
        return;
    }
    let next = form.mode.map_or(modes[0], |current| {
        let idx = modes.iter().position(|mode| *mode == current).unwrap_or(0);
        modes[(idx + 1) % modes.len()]
    });
    form.set_mode(next);
}

/// Public seam for the settings error-popup dismiss in `input/mod.rs`:
/// re-mount the stashed auth form after a token-generate mint failure so
/// the operator lands back on the Edit-auth dialog (parallel to the
/// editor's `Modal::ErrorPopup` recovery).
pub(super) fn restore_settings_auth_form_after_error(
    auth: &mut super::super::state::SettingsAuthState,
) {
    restore_settings_auth_form(auth);
}

fn restore_settings_auth_form(auth: &mut super::super::state::SettingsAuthState) {
    if let Some(AuthFormReturnPath {
        target,
        state,
        focus,
        literal_buffer,
    }) = auth.pending_auth_form_return.take()
    {
        auth.modal = Some(SettingsAuthModal::AuthForm {
            target,
            state,
            focus,
            literal_buffer,
        });
    }
}

/// Lift the stashed settings auth form, apply a literal credential, and
/// re-mount it with focus on Save. Shared by the provide-path
/// `TextInput` commit and the post-mint plain-text generate re-mount in
/// the `run_console` loop — both stage a literal and drop the operator
/// onto Save so the editor's normal save persists it.
pub(in crate::console) fn apply_plain_text_to_settings_auth_form(
    auth: &mut super::super::state::SettingsAuthState,
    value: &str,
) {
    let Some(AuthFormReturnPath {
        target, mut state, ..
    }) = auth.pending_auth_form_return.take()
    else {
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
pub(in crate::console) fn apply_op_picker_to_settings_auth_form(
    auth: &mut super::super::state::SettingsAuthState,
    op_ref: crate::operator_env::OpRef,
) {
    let runner = crate::operator_env::OpCli::new().with_account(op_ref.account.clone());
    apply_op_picker_to_settings_auth_form_with_runner(auth, op_ref, &runner);
}

/// Inner helper split out so tests can inject a fake `OpRunner` without
/// touching the real `op` binary (mirrors
/// `auth::apply_op_picker_to_auth_form_with_runner`).
fn apply_op_picker_to_settings_auth_form_with_runner<R: crate::operator_env::OpRunner + ?Sized>(
    auth: &mut super::super::state::SettingsAuthState,
    op_ref: crate::operator_env::OpRef,
    runner: &R,
) {
    let Some(AuthFormReturnPath {
        target,
        mut state,
        focus,
        literal_buffer,
    }) = auth.pending_auth_form_return.take()
    else {
        return;
    };
    match state.try_commit_op_ref(runner, op_ref) {
        Ok(()) => {
            auth.modal = Some(SettingsAuthModal::AuthForm {
                target,
                state,
                focus: AuthFormFocus::Save,
                literal_buffer,
            });
        }
        Err(err) => {
            // `try_commit_op_ref` mutates `state` only on Ok, so the
            // credential is untouched; re-stash so a later restore lands
            // the operator back on the form with the prior value.
            auth.pending_auth_form_return = Some(AuthFormReturnPath {
                target,
                state,
                focus,
                literal_buffer,
            });
            auth.error = Some(format!("1Password read failed: {err}"));
        }
    }
}

fn persist_settings_auth_form(
    auth: &mut super::super::state::SettingsAuthState,
    env: &mut super::super::state::SettingsEnvState<'_>,
    form: &AuthForm,
) {
    let Some(outcome) = form.commit() else {
        return;
    };
    if let Some(row) = auth.pending.iter_mut().find(|row| row.kind == form.kind) {
        row.mode = outcome.mode;
    }
    if let (Some(name), Some(value)) = (outcome.env_var_name, outcome.env_value) {
        match form.kind {
            crate::console::manager::auth_kind::AuthKind::Github => {
                auth.github_env.insert(name.to_string(), value);
            }
            crate::console::manager::auth_kind::AuthKind::Claude
            | crate::console::manager::auth_kind::AuthKind::Codex
            | crate::console::manager::auth_kind::AuthKind::Amp
            | crate::console::manager::auth_kind::AuthKind::Kimi
            | crate::console::manager::auth_kind::AuthKind::Opencode => {
                env.pending.env.insert(name.to_string(), value);
            }
        }
    }
    auth.selected = auth
        .selected
        .min(settings_auth_row_count(auth).saturating_sub(1));
}

fn clear_settings_auth_kind(
    auth: &mut super::super::state::SettingsAuthState,
    env: &mut super::super::state::SettingsEnvState<'_>,
    target: &AuthFormTarget,
) {
    let AuthFormTarget::Workspace { kind } = target else {
        return;
    };
    if let Some(row) = auth.pending.iter_mut().find(|row| row.kind == *kind) {
        row.mode = crate::console::manager::auth_kind::AuthMode::Sync;
    }
    for mode in kind.supported_modes() {
        if let Some(env_var) = kind.required_env_var(*mode) {
            match kind {
                crate::console::manager::auth_kind::AuthKind::Github => {
                    auth.github_env.remove(env_var);
                }
                crate::console::manager::auth_kind::AuthKind::Claude
                | crate::console::manager::auth_kind::AuthKind::Codex
                | crate::console::manager::auth_kind::AuthKind::Amp
                | crate::console::manager::auth_kind::AuthKind::Kimi
                | crate::console::manager::auth_kind::AuthKind::Opencode => {
                    env.pending.env.remove(env_var);
                }
            }
        }
    }
}

fn handle_general_key(state: &mut ManagerState<'_>, key: KeyEvent) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    match key.code {
        KeyCode::Esc | KeyCode::Char('q' | 'Q') => {
            if settings.is_dirty() {
                settings.mounts.modal = Some(confirm_modal(GlobalMountConfirm::Discard));
            } else {
                state.stage = ManagerStage::List;
            }
        }
        KeyCode::Up if settings.general.selected > 0 => {
            settings.general.selected -= 1;
        }
        KeyCode::Down if settings.general.selected < 1 => {
            settings.general.selected += 1;
        }
        // Space is the W3C toggle key (switch pattern).
        KeyCode::Char(' ') => match settings.general.selected {
            0 => {
                settings.general.pending_coauthor_trailer =
                    !settings.general.pending_coauthor_trailer;
            }
            1 => {
                settings.general.pending_dco = !settings.general.pending_dco;
            }
            _ => {}
        },
        KeyCode::Char('s' | 'S') => {
            open_settings_save_preview(settings);
        }
        _ => {}
    }
}

fn handle_trust_key(state: &mut ManagerState<'_>, key: KeyEvent) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    let trust = &mut settings.trust;
    match key.code {
        KeyCode::Esc | KeyCode::Char('q' | 'Q') => {
            if settings.is_dirty() {
                settings.mounts.modal = Some(confirm_modal(GlobalMountConfirm::Discard));
            } else {
                state.stage = ManagerStage::List;
            }
        }
        KeyCode::Up | KeyCode::Char('k' | 'K') => {
            trust.selected = trust.selected.saturating_sub(1);
            trust.scroll_y = super::super::render::cursor_scroll_for_panel(
                trust.selected,
                trust.scroll_y,
                state.cached_term_size,
            );
        }
        KeyCode::Down | KeyCode::Char('j' | 'J') => {
            trust.selected = (trust.selected + 1).min(trust.pending.len().saturating_sub(1));
            trust.scroll_y = super::super::render::cursor_scroll_for_panel(
                trust.selected,
                trust.scroll_y,
                state.cached_term_size,
            );
        }
        KeyCode::Char('h' | 'H') => {
            apply_scroll_delta(&mut trust.scroll_x, -8);
        }
        KeyCode::Char('l' | 'L') => {
            apply_scroll_delta(&mut trust.scroll_x, 8);
        }
        // Space is the W3C toggle key (checkbox/switch pattern). Enter is for actions.
        KeyCode::Char(' ') => {
            if let Some(row) = trust.pending.get_mut(trust.selected) {
                row.trusted = !row.trusted;
            }
        }
        KeyCode::Char('s' | 'S') => {
            open_settings_save_preview(settings);
        }
        _ => {}
    }
}

#[allow(clippy::too_many_lines)]
pub(super) fn handle_settings_confirm_modal(
    settings: &mut super::super::state::SettingsState<'_>,
    config: &mut AppConfig,
    paths: &JackinPaths,
    key: KeyEvent,
) {
    let Some(modal) = settings.mounts.modal.take() else {
        return;
    };
    match modal {
        GlobalMountModal::Text { target, mut state } => match state.handle_key(key) {
            ModalOutcome::Commit(value) => {
                let committed_target = target.clone();
                settings.mounts.modal = Some(GlobalMountModal::Text { target, state });
                commit_text(&mut settings.mounts, &committed_target, &value);
            }
            ModalOutcome::Cancel => {
                settings.mounts.pop_modal_chain();
                if settings.mounts.modal.is_none() && settings.mounts.add_draft.take().is_some() {
                    settings.mounts.error = Some("Add mount cancelled.".to_string());
                }
            }
            ModalOutcome::Continue => {
                settings.mounts.modal = Some(GlobalMountModal::Text { target, state });
            }
        },
        GlobalMountModal::FileBrowser { mut state } => match state.handle_key(key) {
            ModalOutcome::Commit(path) => {
                let src = path.display().to_string();
                if let Some(draft) = settings.mounts.add_draft.as_mut() {
                    draft.src.clone_from(&src);
                }
                settings.mounts.modal = Some(GlobalMountModal::FileBrowser { state });
                settings
                    .mounts
                    .open_sub_modal(GlobalMountModal::MountDstChoice {
                        state: crate::console::widgets::mount_dst_choice::MountDstChoiceState::new(
                            src,
                        ),
                    });
            }
            ModalOutcome::Cancel => {
                settings.mounts.pop_modal_chain();
                if settings.mounts.modal.is_none() {
                    settings.mounts.add_draft = None;
                }
            }
            ModalOutcome::Continue => {
                settings.mounts.modal = Some(GlobalMountModal::FileBrowser { state });
            }
        },
        GlobalMountModal::MountDstChoice { mut state } => {
            use crate::console::widgets::mount_dst_choice::MountDstChoice;
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
                    settings.mounts.open_sub_modal(text_modal(
                        GlobalMountTextTarget::AddDestination,
                        "Destination",
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
                commit_add_scope_choice(settings, choice);
            }
            ModalOutcome::Cancel => {
                settings.mounts.pop_modal_chain();
                if settings.mounts.modal.is_none() && settings.mounts.add_draft.take().is_some() {
                    settings.mounts.error = Some("Add mount cancelled.".to_string());
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
                    open_global_mount_file_browser(&mut settings.mounts);
                } else {
                    settings.mounts.error = Some(ADD_DRAFT_LOST.into());
                }
            }
            ModalOutcome::Cancel => {
                settings.mounts.pop_modal_chain();
                if settings.mounts.modal.is_none() && settings.mounts.add_draft.take().is_some() {
                    settings.mounts.error = Some("Add mount cancelled.".to_string());
                }
            }
            ModalOutcome::Continue => {
                settings.mounts.modal = Some(GlobalMountModal::RolePicker { state: picker });
            }
        },
        GlobalMountModal::Confirm { action, mut state } => match state.handle_key(key) {
            ModalOutcome::Commit(true) => commit_settings_confirm(settings, action, config, paths),
            ModalOutcome::Commit(false) | ModalOutcome::Cancel => {
                if matches!(action, GlobalMountConfirm::Sensitive) {
                    settings.mounts.error =
                        Some("Save aborted: sensitive paths not confirmed.".into());
                }
                settings.mounts.clear_modal_chain();
            }
            ModalOutcome::Continue => {
                settings.mounts.modal = Some(GlobalMountModal::Confirm { action, state });
            }
        },
        GlobalMountModal::PreviewSave { mut state } => match state.handle_key(key) {
            ModalOutcome::Commit(_) => commit_settings_save(settings, config, paths),
            ModalOutcome::Cancel => settings.mounts.clear_modal_chain(),
            ModalOutcome::Continue => {
                settings.mounts.modal = Some(GlobalMountModal::PreviewSave { state });
            }
        },
    }
}

#[allow(clippy::too_many_lines)]
pub(super) fn handle_settings_env_modal(
    env: &mut super::super::state::SettingsEnvState<'_>,
    key: KeyEvent,
    op_cache: std::rc::Rc<std::cell::RefCell<crate::console::op_cache::OpCache>>,
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
                    env.error = Some("Env edit cancelled.".to_string());
                }
            }
            ModalOutcome::Continue => {
                env.modal = Some(SettingsEnvModal::Text { target, state });
            }
        },
        SettingsEnvModal::SourcePicker { state: mut source } => {
            use crate::console::widgets::source_picker::SourceChoice;
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
                        &format!("Value for {key}"),
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
                            crate::console::widgets::op_picker::OpPickerState::new_with_cache(
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
                crate::console::widgets::op_picker::OpPickerSelection::NewItem { .. }
                | crate::console::widgets::op_picker::OpPickerSelection::EditItemField { .. },
            ) => unreachable!("settings-env OpPicker runs in Browse mode"),
            ModalOutcome::Commit(
                crate::console::widgets::op_picker::OpPickerSelection::Existing(op_ref),
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
                        let label = format!(
                            "New environment key for {}",
                            settings_env_scope_label(&scope)
                        );
                        let state = settings_env_key_input_state(env, &scope, label, "");
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
                    env,
                    &scope,
                    format!("New {role_key} environment key"),
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
                    env.error = Some("Add env cancelled.".to_string());
                }
            }
            ModalOutcome::Continue => {
                env.modal = Some(SettingsEnvModal::RolePicker { state: picker });
            }
        },
        SettingsEnvModal::ScopePicker { mut state } => match state.handle_key(key) {
            ModalOutcome::Commit(choice) => match choice {
                crate::console::widgets::scope_picker::ScopeChoice::AllAgents => {
                    let scope = SettingsEnvScope::Global;
                    let input_state =
                        settings_env_key_input_state(env, &scope, "New global environment key", "");
                    // Don't stash the just-committed ScopePicker as
                    // the Text modal's parent — Esc on Text would
                    // pop back into a consumed picker. Start the
                    // child modal with an empty parent chain.
                    env.open_sub_modal(SettingsEnvModal::Text {
                        target: SettingsEnvTextTarget::EnvKey { scope },
                        state: Box::new(input_state),
                    });
                }
                crate::console::widgets::scope_picker::ScopeChoice::SpecificAgent => {
                    open_settings_env_role_picker(env);
                }
            },
            ModalOutcome::Cancel => {
                env.pop_modal_chain();
                if env.modal.is_none() {
                    env.error = Some("Add env cancelled.".to_string());
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
    settings: &mut super::super::state::SettingsState<'_>,
    action: GlobalMountConfirm,
    config: &mut AppConfig,
    paths: &JackinPaths,
) {
    match action {
        GlobalMountConfirm::Remove => {
            let global = &mut settings.mounts;
            if global.selected < global.pending.len() {
                global.pending.remove(global.selected);
                global.selected = global.selected.min(global.pending.len());
            }
        }
        GlobalMountConfirm::Save => commit_settings_save(settings, config, paths),
        GlobalMountConfirm::Sensitive => {
            open_settings_save_preview(settings);
        }
        GlobalMountConfirm::Discard => {
            settings.discard();
            settings.mounts.exit_requested = true;
        }
    }
}

fn commit_settings_save(
    settings: &mut super::super::state::SettingsState<'_>,
    config: &mut AppConfig,
    paths: &JackinPaths,
) {
    match settings.save_to_config(paths) {
        Ok(saved) => {
            *config = saved;
            settings.mounts.exit_requested = true;
        }
        Err(err) => settings.mounts.error = Some(err.to_string()),
    }
}

fn open_settings_save_preview(settings: &mut super::super::state::SettingsState<'_>) {
    let lines = super::save::build_settings_save_lines(settings);
    settings.mounts.modal = Some(super::super::state::GlobalMountModal::PreviewSave {
        state: crate::console::widgets::confirm_save::ConfirmSaveState::new(lines),
    });
}

fn commit_text(
    global: &mut super::super::state::GlobalMountsState<'_>,
    target: &GlobalMountTextTarget,
    value: &str,
) {
    let trimmed = value.trim();
    match target {
        GlobalMountTextTarget::AddScope => {
            commit_add_scope_text(global, trimmed);
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
                global.error = Some(MOUNT_GONE.into());
                return;
            };
            row.mount.src = resolve_path(trimmed);
            global.clear_modal_chain();
        }
        GlobalMountTextTarget::Destination => {
            let Some(row) = global.pending.get_mut(global.selected) else {
                global.error = Some(MOUNT_GONE.into());
                return;
            };
            row.mount.dst = trimmed.to_string();
            global.clear_modal_chain();
        }
        GlobalMountTextTarget::Scope => {
            let Some(row) = global.pending.get_mut(global.selected) else {
                global.error = Some(MOUNT_GONE.into());
                return;
            };
            row.scope = scope_value(trimmed);
            global.clear_modal_chain();
        }
        GlobalMountTextTarget::Rename => {
            if trimmed.is_empty() {
                global.error = Some(MOUNT_NAME_EMPTY.into());
                return;
            }
            let Some(row) = global.pending.get_mut(global.selected) else {
                global.error = Some(MOUNT_GONE.into());
                return;
            };
            row.name = trimmed.to_string();
            global.clear_modal_chain();
        }
    }
}

fn commit_env_text(
    env: &mut super::super::state::SettingsEnvState<'_>,
    target: &SettingsEnvTextTarget,
    value: &str,
) {
    let trimmed = value.trim();
    match target {
        SettingsEnvTextTarget::EnvKey { scope } => {
            if trimmed.is_empty() {
                env.error = Some("Env key cannot be empty.".into());
                let state = settings_env_key_input_state(env, scope, "Key cannot be empty", "");
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
                state: crate::console::widgets::source_picker::SourcePickerState::new(key, true),
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

fn open_settings_env_role_picker(env: &mut super::super::state::SettingsEnvState<'_>) {
    use crate::console::widgets::role_picker::RolePickerState;
    use crate::selector::RoleSelector;

    let roles = env
        .pending
        .roles
        .keys()
        .filter_map(|role| RoleSelector::parse(role).ok())
        .collect::<Vec<_>>();
    if roles.is_empty() {
        env.error = Some("No registered roles available.".into());
        return;
    }
    env.open_sub_modal(SettingsEnvModal::RolePicker {
        state: RolePickerState::with_confirm_label(roles, "select"),
    });
}

fn commit_add_scope_text(global: &mut super::super::state::GlobalMountsState<'_>, value: &str) {
    let Some(draft) = global.add_draft.as_mut() else {
        global.error = Some(ADD_DRAFT_LOST.into());
        return;
    };
    draft.scope = scope_value(value);
    open_global_mount_file_browser(global);
}

fn commit_add_name_text(global: &mut super::super::state::GlobalMountsState<'_>, value: &str) {
    if value.is_empty() {
        global.error = Some(MOUNT_NAME_EMPTY.into());
        global.modal = Some(text_modal(GlobalMountTextTarget::AddName, "Mount name", ""));
        return;
    }
    let Some(draft) = global.add_draft.as_mut() else {
        global.error = Some(ADD_DRAFT_LOST.into());
        return;
    };
    draft.name = value.to_string();
    global.open_sub_modal(text_modal(GlobalMountTextTarget::AddSource, "Source", ""));
}

fn commit_add_source_text(global: &mut super::super::state::GlobalMountsState<'_>, value: &str) {
    let Some(draft) = global.add_draft.as_mut() else {
        global.error = Some(ADD_DRAFT_LOST.into());
        return;
    };
    draft.src = resolve_path(value);
    global.open_sub_modal(text_modal(
        GlobalMountTextTarget::AddDestination,
        "Destination",
        "",
    ));
}

fn commit_add_destination_text(
    global: &mut super::super::state::GlobalMountsState<'_>,
    value: &str,
) {
    let Some(draft) = global.add_draft.as_mut() else {
        global.error = Some(ADD_DRAFT_LOST.into());
        return;
    };
    draft.dst = value.to_string();
    finalize_global_mount_add(global);
}

fn open_global_mount_scope_picker(global: &mut super::super::state::GlobalMountsState<'_>) {
    global.add_draft = Some(GlobalMountDraft::default());
    global.modal_parents.clear();
    global.modal = Some(scope_picker_modal());
}

fn open_global_mount_file_browser(global: &mut super::super::state::GlobalMountsState<'_>) {
    match FileBrowserState::new_from_home() {
        Ok(state) => {
            global.open_sub_modal(GlobalMountModal::FileBrowser {
                state: Box::new(state),
            });
        }
        Err(err) => {
            global.add_draft = None;
            global.error = Some(err.to_string());
        }
    }
}

fn finalize_global_mount_add(global: &mut super::super::state::GlobalMountsState<'_>) {
    let Some(mut draft) = global.add_draft.take() else {
        global.error = Some(ADD_DRAFT_LOST.into());
        return;
    };
    if draft.dst.trim().is_empty() {
        global.error = Some("Mount destination cannot be empty.".into());
        global.add_draft = Some(draft);
        return;
    }
    draft.name = unique_global_mount_name(global, &draft);
    global.pending.push(crate::config::GlobalMountRow {
        scope: draft.scope,
        name: draft.name,
        mount: MountConfig {
            src: draft.src,
            dst: draft.dst,
            readonly: false,
            isolation: crate::isolation::MountIsolation::Shared,
        },
    });
    global.selected = global.pending.len().saturating_sub(1);
    global.clear_modal_chain();
}

fn unique_global_mount_name(
    global: &super::super::state::GlobalMountsState<'_>,
    draft: &GlobalMountDraft,
) -> String {
    let basename = std::path::Path::new(&draft.dst)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("mount");
    let base = basename
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    let base = if base.is_empty() {
        "mount".to_string()
    } else {
        base
    };
    let mut candidate = base.clone();
    let mut suffix = 2;
    while global
        .pending
        .iter()
        .any(|row| row.scope == draft.scope && row.name == candidate)
    {
        candidate = format!("{base}-{suffix}");
        suffix += 1;
    }
    candidate
}

fn open_edit_text(state: &mut ManagerState<'_>, target: GlobalMountTextTarget) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    let global = &mut settings.mounts;
    let Some(row) = global.pending.get(global.selected) else {
        return;
    };
    let (label, initial) = match target {
        GlobalMountTextTarget::Rename => ("Rename mount", row.name.clone()),
        GlobalMountTextTarget::Source => ("Source", row.mount.src.clone()),
        GlobalMountTextTarget::Destination => ("Destination", row.mount.dst.clone()),
        GlobalMountTextTarget::Scope => (
            "Scope (empty = global)",
            row.scope.clone().unwrap_or_default(),
        ),
        // Add-flow targets are driven by the four-step text wizard, not this entry point.
        GlobalMountTextTarget::AddScope
        | GlobalMountTextTarget::AddName
        | GlobalMountTextTarget::AddSource
        | GlobalMountTextTarget::AddDestination => return,
    };
    global.modal = Some(text_modal(target, label, &initial));
}

fn open_settings_env_enter_modal(settings: &mut super::super::state::SettingsState<'_>) {
    let rows = settings_env_flat_rows(settings);
    let Some(row) = rows.get(settings.env.selected).cloned() else {
        return;
    };
    match row {
        SettingsEnvRow::Key { scope, key } => {
            if settings_env_value(&settings.env, &scope, &key)
                .is_some_and(|v| matches!(v, crate::operator_env::EnvValue::OpRef(_)))
            {
                return;
            }
            let current = settings_env_value(&settings.env, &scope, &key)
                .map(|v| v.as_persisted_str().to_string())
                .unwrap_or_default();
            settings.env.modal = Some(SettingsEnvModal::Text {
                target: SettingsEnvTextTarget::EnvValue {
                    scope,
                    key: key.clone(),
                },
                state: Box::new(TextInputState::new_allow_empty(
                    format!("Edit {key}"),
                    current,
                )),
            });
        }
        SettingsEnvRow::GlobalAddSentinel => {
            settings.env.modal = Some(SettingsEnvModal::ScopePicker {
                state: crate::console::widgets::scope_picker::ScopePickerState::new(),
            });
        }
        SettingsEnvRow::RoleHeader { role, expanded } => {
            if !expanded {
                settings.env.expanded.insert(role);
            }
        }
        SettingsEnvRow::RoleAddSentinel(role) => {
            let scope = SettingsEnvScope::Role(role.clone());
            let label = format!("New {role} environment key");
            let state = settings_env_key_input_state(&settings.env, &scope, label, "");
            settings.env.modal = Some(SettingsEnvModal::Text {
                target: SettingsEnvTextTarget::EnvKey { scope },
                state: Box::new(state),
            });
        }
        SettingsEnvRow::SectionSpacer => {}
    }
}

fn open_settings_env_add_modal(settings: &mut super::super::state::SettingsState<'_>) {
    let rows = settings_env_flat_rows(settings);
    let Some(row) = rows.get(settings.env.selected).cloned() else {
        return;
    };
    let (scope, label) = match row {
        SettingsEnvRow::Key {
            scope: SettingsEnvScope::Global,
            ..
        }
        | SettingsEnvRow::GlobalAddSentinel => (
            SettingsEnvScope::Global,
            "New global environment key".to_string(),
        ),
        SettingsEnvRow::RoleHeader { role, .. }
        | SettingsEnvRow::Key {
            scope: SettingsEnvScope::Role(role),
            ..
        }
        | SettingsEnvRow::RoleAddSentinel(role) => (
            SettingsEnvScope::Role(role.clone()),
            format!("New {role} environment key"),
        ),
        SettingsEnvRow::SectionSpacer => return,
    };
    let state = settings_env_key_input_state(&settings.env, &scope, label, "");
    settings.env.modal = Some(SettingsEnvModal::Text {
        target: SettingsEnvTextTarget::EnvKey { scope },
        state: Box::new(state),
    });
}

fn open_settings_env_delete_confirm(settings: &mut super::super::state::SettingsState<'_>) {
    let rows = settings_env_flat_rows(settings);
    let Some(SettingsEnvRow::Key { key, .. }) = rows.get(settings.env.selected).cloned() else {
        return;
    };
    settings.env.modal = Some(SettingsEnvModal::Confirm {
        action: SettingsEnvConfirm::Delete,
        state: ConfirmState::new(format!("Delete environment variable {key}?")),
    });
}

fn toggle_settings_env_mask(settings: &mut super::super::state::SettingsState<'_>) {
    let rows = settings_env_flat_rows(settings);
    let Some(SettingsEnvRow::Key { scope, key }) = rows.get(settings.env.selected).cloned() else {
        return;
    };
    if settings_env_value(&settings.env, &scope, &key)
        .is_some_and(|v| matches!(v, crate::operator_env::EnvValue::OpRef(_)))
    {
        return;
    }
    let tag = (scope, key);
    if !settings.env.unmasked_rows.remove(&tag) {
        settings.env.unmasked_rows.insert(tag);
    }
}

fn open_settings_env_picker_modal(
    settings: &mut super::super::state::SettingsState<'_>,
    op_cache: std::rc::Rc<std::cell::RefCell<crate::console::op_cache::OpCache>>,
) {
    let rows = settings_env_flat_rows(settings);
    let Some(row) = rows.get(settings.env.selected).cloned() else {
        return;
    };
    let target = match row {
        SettingsEnvRow::Key { scope, key } => Some((scope, Some(key))),
        SettingsEnvRow::GlobalAddSentinel => Some((SettingsEnvScope::Global, None)),
        SettingsEnvRow::RoleAddSentinel(role) => Some((SettingsEnvScope::Role(role), None)),
        SettingsEnvRow::RoleHeader { .. } | SettingsEnvRow::SectionSpacer => None,
    };
    let Some(target) = target else {
        return;
    };
    settings.env.pending_picker_target = Some(target);
    settings.env.modal = Some(SettingsEnvModal::OpPicker {
        state: Box::new(
            crate::console::widgets::op_picker::OpPickerState::new_with_cache(op_cache),
        ),
    });
}

fn delete_selected_settings_env(env: &mut super::super::state::SettingsEnvState<'_>) {
    let rows = settings_env_rows_from_env(env);
    if let Some(SettingsEnvRow::Key { scope, key }) = rows.get(env.selected).cloned() {
        match scope {
            SettingsEnvScope::Global => {
                env.pending.env.remove(&key);
            }
            SettingsEnvScope::Role(role) => {
                if let Some(role_env) = env.pending.roles.get_mut(&role) {
                    role_env.remove(&key);
                }
            }
        }
        let row_count = settings_env_rows_from_env(env).len();
        env.selected = env.selected.min(row_count.saturating_sub(1));
    }
}

fn settings_env_rows_from_env(
    env: &super::super::state::SettingsEnvState<'_>,
) -> Vec<SettingsEnvRow> {
    let mut rows = Vec::new();
    for key in env.pending.env.keys() {
        rows.push(SettingsEnvRow::Key {
            scope: SettingsEnvScope::Global,
            key: key.clone(),
        });
    }
    if !env.pending.env.is_empty() {
        rows.push(SettingsEnvRow::SectionSpacer);
    }
    rows.push(SettingsEnvRow::GlobalAddSentinel);
    for (role, role_env) in &env.pending.roles {
        if role_env.is_empty() {
            continue;
        }
        rows.push(SettingsEnvRow::SectionSpacer);
        let expanded = env.expanded.contains(role);
        rows.push(SettingsEnvRow::RoleHeader {
            role: role.clone(),
            expanded,
        });
        if expanded {
            if let Some(role_env) = env.pending.roles.get(role) {
                for key in role_env.keys() {
                    rows.push(SettingsEnvRow::Key {
                        scope: SettingsEnvScope::Role(role.clone()),
                        key: key.clone(),
                    });
                }
            }
            rows.push(SettingsEnvRow::SectionSpacer);
            rows.push(SettingsEnvRow::RoleAddSentinel(role.clone()));
        }
    }
    rows
}

fn settings_env_value<'a>(
    env: &'a super::super::state::SettingsEnvState<'_>,
    scope: &SettingsEnvScope,
    key: &str,
) -> Option<&'a crate::operator_env::EnvValue> {
    match scope {
        SettingsEnvScope::Global => env.pending.env.get(key),
        SettingsEnvScope::Role(role) => env
            .pending
            .roles
            .get(role)
            .and_then(|role_env| role_env.get(key)),
    }
}

const fn settings_env_scope_label(scope: &SettingsEnvScope) -> &str {
    match scope {
        SettingsEnvScope::Global => "global",
        SettingsEnvScope::Role(role) => role.as_str(),
    }
}

fn forbidden_settings_env_keys(
    env: &super::super::state::SettingsEnvState<'_>,
    scope: &SettingsEnvScope,
) -> Vec<String> {
    match scope {
        SettingsEnvScope::Global => env.pending.env.keys().cloned().collect(),
        SettingsEnvScope::Role(role) => env
            .pending
            .roles
            .get(role)
            .map(|role_env| role_env.keys().cloned().collect())
            .unwrap_or_default(),
    }
}

fn forbidden_settings_env_label(scope: &SettingsEnvScope) -> String {
    match scope {
        SettingsEnvScope::Global => "global env".to_string(),
        SettingsEnvScope::Role(role) => format!("role {role}"),
    }
}

fn settings_env_key_input_state<'a>(
    env: &super::super::state::SettingsEnvState<'_>,
    scope: &SettingsEnvScope,
    label: impl Into<String>,
    initial: impl Into<String>,
) -> TextInputState<'a> {
    let mut state =
        TextInputState::new_with_forbidden(label, initial, forbidden_settings_env_keys(env, scope));
    state.forbidden_label = forbidden_settings_env_label(scope);
    state
}

fn set_settings_env_value_typed(
    env: &mut super::super::state::SettingsEnvState<'_>,
    scope: &SettingsEnvScope,
    key: &str,
    value: crate::operator_env::EnvValue,
) {
    match scope {
        SettingsEnvScope::Global => {
            env.pending.env.insert(key.to_string(), value);
        }
        SettingsEnvScope::Role(role) => {
            env.pending
                .roles
                .entry(role.clone())
                .or_default()
                .insert(key.to_string(), value);
            env.expanded.insert(role.clone());
        }
    }
}

/// Promote any pending error from a settings sub-tab to `settings.error_popup`,
/// pop back to the workspace list when a handler set `exit_requested`.
pub(super) fn after_settings_event(state: &mut ManagerState<'_>) {
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
    if let Some(msg) = error {
        settings.error_popup = Some(crate::console::widgets::error_popup::ErrorPopupState::new(
            "Settings error",
            msg,
        ));
    }
    if exit {
        state.stage = ManagerStage::List;
    }
}

fn confirm_modal(action: GlobalMountConfirm) -> GlobalMountModal<'static> {
    let prompt = match action {
        GlobalMountConfirm::Save => "Save settings to ~/.config/jackin/config.toml?",
        GlobalMountConfirm::Sensitive => "Sensitive global mount path detected. Save anyway?",
        GlobalMountConfirm::Remove => "Remove selected global mount?",
        GlobalMountConfirm::Discard => "Discard unsaved global mount changes?",
    };
    GlobalMountModal::Confirm {
        action,
        state: ConfirmState::new(prompt),
    }
}

const fn scope_picker_modal() -> GlobalMountModal<'static> {
    GlobalMountModal::ScopePicker {
        state: crate::console::widgets::scope_picker::ScopePickerState::with_title(
            " Which agent role do you want to add? ",
        ),
    }
}

fn commit_add_scope_choice(
    settings: &mut super::super::state::SettingsState<'_>,
    choice: crate::console::widgets::scope_picker::ScopeChoice,
) {
    match choice {
        crate::console::widgets::scope_picker::ScopeChoice::AllAgents => {
            commit_text(&mut settings.mounts, &GlobalMountTextTarget::AddScope, "");
        }
        crate::console::widgets::scope_picker::ScopeChoice::SpecificAgent => {
            open_global_mount_role_picker(settings);
        }
    }
}

fn open_global_mount_role_picker(settings: &mut super::super::state::SettingsState<'_>) {
    let roles = settings
        .trust
        .pending
        .iter()
        .filter_map(|row| RoleSelector::parse(&row.role).ok())
        .collect::<Vec<_>>();
    if roles.is_empty() {
        settings.mounts.error = Some("No registered roles available.".into());
        return;
    }
    settings
        .mounts
        .open_sub_modal(GlobalMountModal::RolePicker {
            state: RolePickerState::with_confirm_label(roles, "select"),
        });
}

fn text_modal(
    target: GlobalMountTextTarget,
    label: &str,
    initial: &str,
) -> GlobalMountModal<'static> {
    GlobalMountModal::Text {
        target,
        state: Box::new(TextInputState::new(label, initial)),
    }
}

fn env_text_modal(
    target: SettingsEnvTextTarget,
    label: &str,
    initial: &str,
) -> SettingsEnvModal<'static> {
    let state = if matches!(target, SettingsEnvTextTarget::EnvValue { .. }) {
        TextInputState::new_allow_empty(label, initial)
    } else {
        TextInputState::new(label, initial)
    };
    SettingsEnvModal::Text {
        target,
        state: Box::new(state),
    }
}

fn scope_value(value: &str) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn has_sensitive_mount(rows: &[crate::config::GlobalMountRow]) -> bool {
    let mounts: Vec<MountConfig> = rows.iter().map(|row| row.mount.clone()).collect();
    !crate::workspace::find_sensitive_mounts(&mounts).is_empty()
}

#[cfg(test)]
mod tests {
    use super::super::super::state::{
        ManagerStage, ManagerState, SettingsEnvModal, SettingsEnvTextTarget, SettingsState,
        SettingsTab,
    };
    use super::super::test_support::key;
    use super::*;
    use crate::config::RoleSource;
    use std::collections::BTreeMap;

    #[test]
    fn global_mount_save_detects_sensitive_sources() {
        let rows = vec![crate::config::GlobalMountRow {
            scope: None,
            name: "ssh".into(),
            mount: MountConfig {
                src: "/home/user/.ssh".into(),
                dst: "/ssh".into(),
                readonly: true,
                isolation: crate::isolation::MountIsolation::Shared,
            },
        }];

        assert!(has_sensitive_mount(&rows));
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

        handle_settings_confirm_modal(settings, &mut config, &paths, key(KeyCode::Enter));
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
        handle_settings_confirm_modal(settings, &mut config, &paths, key(KeyCode::Enter));
        assert!(matches!(
            settings.mounts.modal,
            Some(GlobalMountModal::FileBrowser { .. })
        ));

        handle_settings_confirm_modal(settings, &mut config, &paths, key(KeyCode::Esc));

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
        picker.focused = crate::console::widgets::scope_picker::ScopeChoice::SpecificAgent;
        handle_settings_confirm_modal(settings, &mut config, &paths, key(KeyCode::Enter));
        assert!(matches!(
            settings.mounts.modal,
            Some(GlobalMountModal::RolePicker { .. })
        ));

        handle_settings_confirm_modal(settings, &mut config, &paths, key(KeyCode::Enter));
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
        picker.focused = crate::console::widgets::scope_picker::ScopeChoice::SpecificAgent;
        handle_settings_confirm_modal(settings, &mut config, &paths, key(KeyCode::Enter));
        assert!(matches!(
            settings.mounts.modal,
            Some(GlobalMountModal::RolePicker { .. })
        ));

        handle_settings_confirm_modal(settings, &mut config, &paths, key(KeyCode::Esc));

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
            crate::console::manager::auth_kind::AuthMode::Sync
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
        use crate::console::manager::auth_kind::{AuthKind, AuthMode};

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
            crate::console::op_cache::OpCache::default(),
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
            settings.auth.pending_auth_form_return.is_some(),
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
        use crate::console::manager::auth_kind::{AuthKind, AuthMode};
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
            crate::console::op_cache::OpCache::default(),
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
        assert!(settings.auth.pending_auth_form_return.is_some());

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
        assert!(settings.auth.pending_auth_form_return.is_none());
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
        use crate::console::manager::auth_kind::{AuthKind, AuthMode};

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
            crate::console::op_cache::OpCache::default(),
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
                    scope: super::super::super::state::SettingsEnvScope::Global
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
            scope: super::super::super::state::SettingsEnvScope::Global,
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
        picker.focused = crate::console::widgets::scope_picker::ScopeChoice::SpecificAgent;
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
                    scope: super::super::super::state::SettingsEnvScope::Role(role)
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
    fn after_settings_event_promotes_mounts_error_to_error_popup() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let config = AppConfig::default();
        let mut state = ManagerState::from_config(&config, tmp.path());
        let mut settings = SettingsState::from_config(&config);
        settings.mounts.error = Some("mount error detail".into());
        state.stage = ManagerStage::Settings(settings);

        after_settings_event(&mut state);

        let ManagerStage::Settings(settings) = &state.stage else {
            panic!("must stay in Settings stage");
        };
        assert!(
            settings.error_popup.is_some(),
            "error_popup must be set after after_settings_event"
        );
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
