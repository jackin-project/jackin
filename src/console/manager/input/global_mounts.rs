use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::super::render::global_mounts::{SettingsEnvRow, settings_env_flat_rows};
use super::super::state::{
    AuthFormFocus, AuthFormReturnPath, AuthFormTarget, GlobalMountConfirm, GlobalMountDraft,
    GlobalMountModal, GlobalMountTextTarget, ManagerStage, ManagerState, SettingsAuthModal,
    SettingsEnvConfirm, SettingsEnvModal, SettingsEnvScope, SettingsEnvTextTarget, SettingsTab,
    Toast, ToastKind,
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

const NO_MOUNT_SELECTED: &str = "No mount selected.";
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
            global.scroll_x = global.scroll_x.saturating_sub(8);
        }
        KeyCode::Char('l' | 'L') => {
            global.scroll_x = global.scroll_x.saturating_add(8);
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
        KeyCode::Char('d' | 'D') => {
            if global.pending.is_empty() {
                set_toast(state, "Nothing to remove.", ToastKind::Error);
            } else {
                global.modal = Some(confirm_modal(GlobalMountConfirm::Remove));
            }
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

#[allow(clippy::too_many_lines)]
pub(super) fn handle_settings_auth_modal(
    auth: &mut super::super::state::SettingsAuthState,
    env: &mut super::super::state::SettingsEnvState<'_>,
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
            match state.handle_key(key) {
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
            ModalOutcome::Commit(value) => {
                if let Some(AuthFormReturnPath {
                    target, mut state, ..
                }) = auth.pending_auth_form_return.take()
                {
                    state.set_literal(value.clone());
                    auth.modal = Some(SettingsAuthModal::AuthForm {
                        target,
                        state,
                        focus: AuthFormFocus::Save,
                        literal_buffer: value,
                    });
                }
            }
            ModalOutcome::Cancel => restore_settings_auth_form(auth),
            ModalOutcome::Continue => auth.modal = Some(modal),
        },
        SettingsAuthModal::OpPicker { state } => match state.handle_key(key) {
            ModalOutcome::Commit(op_ref) => {
                if let Some(AuthFormReturnPath {
                    target,
                    mut state,
                    literal_buffer,
                    ..
                }) = auth.pending_auth_form_return.take()
                {
                    match state.try_commit_op_ref(&crate::operator_env::OpCli::new(), op_ref) {
                        Ok(()) => {
                            auth.modal = Some(SettingsAuthModal::AuthForm {
                                target,
                                state,
                                focus: AuthFormFocus::Save,
                                literal_buffer,
                            });
                        }
                        Err(err) => {
                            auth.error = Some(format!("1Password read failed: {err}"));
                        }
                    }
                }
            }
            ModalOutcome::Cancel => restore_settings_auth_form(auth),
            ModalOutcome::Continue => auth.modal = Some(modal),
        },
    }
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
            trust.scroll_x = trust.scroll_x.saturating_sub(8);
        }
        KeyCode::Char('l' | 'L') => {
            trust.scroll_x = trust.scroll_x.saturating_add(8);
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
    let Some(mut modal) = settings.mounts.modal.take() else {
        return;
    };
    match &mut modal {
        GlobalMountModal::Text { target, state } => match state.handle_key(key) {
            ModalOutcome::Commit(value) => commit_text(&mut settings.mounts, target, &value),
            ModalOutcome::Cancel => {
                if settings.mounts.add_draft.take().is_some() {
                    settings.mounts.error = Some("Add mount cancelled.".to_string());
                }
            }
            ModalOutcome::Continue => settings.mounts.modal = Some(modal),
        },
        GlobalMountModal::FileBrowser { state } => match state.handle_key(key) {
            ModalOutcome::Commit(path) => {
                let src = path.display().to_string();
                if let Some(draft) = settings.mounts.add_draft.as_mut() {
                    draft.src.clone_from(&src);
                }
                settings.mounts.modal = Some(GlobalMountModal::MountDstChoice {
                    state: crate::console::widgets::mount_dst_choice::MountDstChoiceState::new(src),
                });
            }
            ModalOutcome::Cancel => {
                settings.mounts.add_draft = None;
            }
            ModalOutcome::Continue => settings.mounts.modal = Some(modal),
        },
        GlobalMountModal::MountDstChoice { state } => {
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
                    settings.mounts.modal = Some(text_modal(
                        GlobalMountTextTarget::AddDestination,
                        "Destination",
                        &src,
                    ));
                }
                ModalOutcome::Cancel => {
                    settings.mounts.add_draft = None;
                }
                ModalOutcome::Continue => settings.mounts.modal = Some(modal),
            }
        }
        GlobalMountModal::ScopePicker { state } => match state.handle_key(key) {
            ModalOutcome::Commit(choice) => commit_add_scope_choice(settings, choice),
            ModalOutcome::Cancel => {
                if settings.mounts.add_draft.take().is_some() {
                    settings.mounts.error = Some("Add mount cancelled.".to_string());
                }
            }
            ModalOutcome::Continue => settings.mounts.modal = Some(modal),
        },
        GlobalMountModal::RolePicker { state: picker } => match picker.handle_key(key) {
            ModalOutcome::Commit(role) => {
                if let Some(draft) = settings.mounts.add_draft.as_mut() {
                    draft.scope = Some(role.key());
                    open_global_mount_file_browser(&mut settings.mounts);
                } else {
                    settings.mounts.error = Some(ADD_DRAFT_LOST.into());
                }
            }
            ModalOutcome::Cancel => {
                if settings.mounts.add_draft.take().is_some() {
                    settings.mounts.error = Some("Add mount cancelled.".to_string());
                }
            }
            ModalOutcome::Continue => settings.mounts.modal = Some(modal),
        },
        GlobalMountModal::Confirm { action, state } => match state.handle_key(key) {
            ModalOutcome::Commit(true) => commit_settings_confirm(settings, *action, config, paths),
            ModalOutcome::Commit(false) | ModalOutcome::Cancel => {
                if matches!(action, GlobalMountConfirm::Sensitive) {
                    settings.mounts.error =
                        Some("Save aborted: sensitive paths not confirmed.".into());
                }
            }
            ModalOutcome::Continue => settings.mounts.modal = Some(modal),
        },
        GlobalMountModal::PreviewSave { state } => match state.handle_key(key) {
            ModalOutcome::Commit(_) => commit_settings_save(settings, config, paths),
            ModalOutcome::Cancel => {} // modal already taken; don't put back
            ModalOutcome::Continue => settings.mounts.modal = Some(modal),
        },
    }
}

#[allow(clippy::too_many_lines)]
pub(super) fn handle_settings_env_modal(
    env: &mut super::super::state::SettingsEnvState<'_>,
    key: KeyEvent,
    op_cache: std::rc::Rc<std::cell::RefCell<crate::console::op_cache::OpCache>>,
) {
    let Some(mut modal) = env.modal.take() else {
        return;
    };
    match &mut modal {
        SettingsEnvModal::Text { target, state } => match state.handle_key(key) {
            ModalOutcome::Commit(value) => commit_env_text(env, target, &value),
            ModalOutcome::Cancel => {
                env.pending_env_key = None;
                env.pending_picker_value = None;
                env.error = Some("Env edit cancelled.".to_string());
            }
            ModalOutcome::Continue => env.modal = Some(modal),
        },
        SettingsEnvModal::SourcePicker { state: source } => {
            use crate::console::widgets::source_picker::SourceChoice;
            match source.handle_key(key) {
                ModalOutcome::Commit(SourceChoice::Plain) => {
                    let Some((scope, key)) = env.pending_env_key.clone() else {
                        env.modal = None;
                        return;
                    };
                    env.modal = Some(env_text_modal(
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
                        env.modal = None;
                        return;
                    };
                    env.pending_picker_target = Some((scope, Some(key)));
                    env.pending_env_key = None;
                    env.modal = Some(SettingsEnvModal::OpPicker {
                        state: Box::new(
                            crate::console::widgets::op_picker::OpPickerState::new_with_cache(
                                op_cache,
                            ),
                        ),
                    });
                }
                ModalOutcome::Cancel => {
                    env.modal = None;
                    env.pending_env_key = None;
                    env.pending_picker_value = None;
                }
                ModalOutcome::Continue => env.modal = Some(modal),
            }
        }
        SettingsEnvModal::OpPicker { state: picker } => match picker.handle_key(key) {
            ModalOutcome::Commit(op_ref) => {
                let target = env.pending_picker_target.take();
                match target {
                    Some((scope, Some(key))) => {
                        set_settings_env_value_typed(
                            env,
                            &scope,
                            &key,
                            crate::operator_env::EnvValue::OpRef(op_ref),
                        );
                        env.modal = None;
                    }
                    Some((scope, None)) => {
                        env.pending_picker_value =
                            Some(crate::operator_env::EnvValue::OpRef(op_ref));
                        let label = format!(
                            "New environment key for {}",
                            settings_env_scope_label(&scope)
                        );
                        let state = settings_env_key_input_state(env, &scope, label, "");
                        env.modal = Some(SettingsEnvModal::Text {
                            target: SettingsEnvTextTarget::EnvKey { scope },
                            state: Box::new(state),
                        });
                    }
                    None => env.modal = None,
                }
            }
            ModalOutcome::Cancel => {
                env.modal = None;
                env.pending_picker_target = None;
                env.pending_picker_value = None;
            }
            ModalOutcome::Continue => env.modal = Some(modal),
        },
        SettingsEnvModal::RolePicker { state: picker } => match picker.handle_key(key) {
            ModalOutcome::Commit(role) => {
                let role_key = role.key();
                let scope = SettingsEnvScope::Role(role_key.clone());
                let state = settings_env_key_input_state(
                    env,
                    &scope,
                    format!("New {role_key} environment key"),
                    "",
                );
                env.modal = Some(SettingsEnvModal::Text {
                    target: SettingsEnvTextTarget::EnvKey { scope },
                    state: Box::new(state),
                });
            }
            ModalOutcome::Cancel => {
                env.error = Some("Add env cancelled.".to_string());
            }
            ModalOutcome::Continue => env.modal = Some(modal),
        },
        SettingsEnvModal::ScopePicker { state } => match state.handle_key(key) {
            ModalOutcome::Commit(choice) => match choice {
                crate::console::widgets::scope_picker::ScopeChoice::AllAgents => {
                    let scope = SettingsEnvScope::Global;
                    let state =
                        settings_env_key_input_state(env, &scope, "New global environment key", "");
                    env.modal = Some(SettingsEnvModal::Text {
                        target: SettingsEnvTextTarget::EnvKey { scope },
                        state: Box::new(state),
                    });
                }
                crate::console::widgets::scope_picker::ScopeChoice::SpecificAgent => {
                    open_settings_env_role_picker(env);
                }
            },
            ModalOutcome::Cancel => {
                env.error = Some("Add env cancelled.".to_string());
            }
            ModalOutcome::Continue => env.modal = Some(modal),
        },
        SettingsEnvModal::Confirm { action, state } => match state.handle_key(key) {
            ModalOutcome::Commit(true) => match action {
                SettingsEnvConfirm::Delete => {
                    delete_selected_settings_env(env);
                }
            },
            ModalOutcome::Commit(false) | ModalOutcome::Cancel => {}
            ModalOutcome::Continue => env.modal = Some(modal),
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
            settings.mounts.success = Some("Settings saved.".into());
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
        }
        GlobalMountTextTarget::Destination => {
            let Some(row) = global.pending.get_mut(global.selected) else {
                global.error = Some(MOUNT_GONE.into());
                return;
            };
            row.mount.dst = trimmed.to_string();
        }
        GlobalMountTextTarget::Scope => {
            let Some(row) = global.pending.get_mut(global.selected) else {
                global.error = Some(MOUNT_GONE.into());
                return;
            };
            row.scope = scope_value(trimmed);
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
                return;
            }
            env.pending_env_key = Some((scope.clone(), key.clone()));
            env.modal = Some(SettingsEnvModal::SourcePicker {
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
    env.modal = Some(SettingsEnvModal::RolePicker {
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
    global.modal = Some(text_modal(GlobalMountTextTarget::AddSource, "Source", ""));
}

fn commit_add_source_text(global: &mut super::super::state::GlobalMountsState<'_>, value: &str) {
    let Some(draft) = global.add_draft.as_mut() else {
        global.error = Some(ADD_DRAFT_LOST.into());
        return;
    };
    draft.src = resolve_path(value);
    global.modal = Some(text_modal(
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
    global.modal = Some(scope_picker_modal());
}

fn open_global_mount_file_browser(global: &mut super::super::state::GlobalMountsState<'_>) {
    match FileBrowserState::new_from_home() {
        Ok(state) => {
            global.modal = Some(GlobalMountModal::FileBrowser {
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
        set_toast(state, NO_MOUNT_SELECTED, ToastKind::Error);
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

/// Promote pending error/success messages to toasts; pop back to the
/// workspace list when the handler set `exit_requested`.
pub(super) fn after_global_mounts_event(state: &mut ManagerState<'_>) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    let global = &mut settings.mounts;
    let error = global.error.take();
    let success = global.success.take();
    let exit = std::mem::take(&mut global.exit_requested);
    if let Some(err) = error {
        set_toast(state, &err, ToastKind::Error);
    } else if let Some(msg) = success {
        set_toast(state, &msg, ToastKind::Success);
    }
    if exit {
        state.stage = ManagerStage::List;
    }
}

pub(super) fn after_settings_event(state: &mut ManagerState<'_>) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    let env_error = settings.env.error.take();
    let auth_error = settings.auth.error.take();
    let trust_error = settings.trust.error.take();
    if let Some(err) = env_error.or(auth_error).or(trust_error) {
        set_toast(state, &err, ToastKind::Error);
    }
    after_global_mounts_event(state);
}

fn set_toast(state: &mut ManagerState<'_>, msg: &str, kind: ToastKind) {
    state.toast = Some(Toast {
        message: msg.to_string(),
        kind,
        shown_at: std::time::Instant::now(),
    });
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
    settings.mounts.modal = Some(GlobalMountModal::RolePicker {
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
        state.stage = ManagerStage::Settings(SettingsState::from_config(&config));

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
        state.stage = ManagerStage::Settings(SettingsState::from_config(&config));

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
            matches!(&state.stage, ManagerStage::Settings(settings) if settings.active_tab == SettingsTab::Mounts)
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
}
