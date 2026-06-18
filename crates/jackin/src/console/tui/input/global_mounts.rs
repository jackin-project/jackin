//! Key handler for the Settings → Global Mounts tab and its modals.
//!
//! Dispatches keyboard events to the add/edit/delete flow for global mount
//! entries and for the auth/env panels that share the Settings screen.
//! Produces `ManagerEffect` values the event loop applies; does not write
//! config directly.
//!
//! Not responsible for: rendering (`jackin-console` settings view) or the
//! save commit path (`console/tui/input/save.rs`).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::console::tui::effect::ManagerEffect;
use crate::console::tui::message::{ManagerMessage, update_manager};
use crate::console::tui::state::{
    AuthForm, AuthFormFocus, AuthFormTarget, GlobalMountConfirm, GlobalMountDraft,
    GlobalMountModal, GlobalMountTextTarget, ManagerStage, ManagerState, RolePickerState,
    SettingsAuthModal, SettingsEnvConfirm, SettingsEnvEnterPlan, SettingsEnvModal, SettingsEnvRow,
    SettingsEnvScope, SettingsEnvTextTarget, SettingsTab,
};
use jackin_config::resolve_path;
use jackin_console::tui::auth_config::{
    apply_settings_auth_env_commit, clear_settings_auth_env_values,
};
use jackin_console::tui::components::auth_panel::{
    AuthFormKeyPlan, auth_credential_input_state, auth_form_key_plan_with_source_folder,
    auth_source_picker_state, generated_token_op_item_name, generated_token_source_picker_state,
};
use jackin_console::tui::components::file_browser::{FileBrowserOutcome, page_rows_for_modal};
use jackin_console::tui::mount_display::settings_global_config_mounts_content_width_with_cache;
use jackin_console::tui::screens::settings::update as settings_update;
use jackin_console::tui::screens::settings::view::{
    global_mount_add_draft_lost_message, global_mount_confirm_state,
    global_mount_destination_empty_message, global_mount_gone_message,
    global_mount_name_empty_message, global_mount_no_github_url_message,
    global_mount_scope_picker_state, global_mount_scope_text_value, global_mount_text_input_state,
    global_mount_text_target_label, settings_auth_op_read_failed_message,
    settings_env_delete_confirm_state, settings_env_empty_key_error_message,
    settings_env_empty_key_label, settings_env_key_input_state,
    settings_env_new_key_after_picker_label, settings_env_new_key_label,
    settings_env_scope_picker_state, settings_env_source_picker_state,
    settings_env_text_input_state, settings_env_value_current_text, settings_env_value_text_label,
    settings_error_popup_title, settings_no_registered_roles_error_message,
    settings_sensitive_paths_not_confirmed_message,
};
use jackin_core::RoleSelector;
use jackin_tui::ModalOutcome;

pub(super) type SettingsModalOutcome = jackin_console::tui::message::ConsoleSettingsModalOutcome;

pub(super) type SettingsAuthOutcome =
    jackin_console::tui::message::ConsoleSettingsAuthOutcome<jackin_core::OpRef>;

#[cfg(test)]
pub(super) fn handle_settings_key(state: &mut ManagerState<'_>, key: KeyEvent) {
    handle_settings_key_with_effects(state, key);
}

#[expect(
    clippy::too_many_lines,
    reason = "pending extraction — tracked in codebase-readability roadmap"
)]
pub(super) fn handle_settings_key_with_effects(state: &mut ManagerState<'_>, key: KeyEvent) {
    let ManagerStage::Settings(settings) = &state.stage else {
        return;
    };

    // W3C ARIA Tabs: when tab_bar_focused, Left/Right cycle tabs and Tab/↓
    // enters the content area.
    if settings.tab_bar_focused() {
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
        KeyCode::Esc if !settings.tab_bar_focused() => {
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
            let rows = settings.env_flat_rows();
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
            let rows = settings.env_flat_rows();
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

#[expect(
    clippy::too_many_lines,
    reason = "pending extraction — tracked in codebase-readability roadmap"
)]
fn handle_global_mounts_key(state: &mut ManagerState<'_>, key: KeyEvent) {
    // S is handled here, before `global` borrows `settings.mounts`, so
    // `open_settings_save_preview` can receive all of `settings`.
    if matches!(key.code, KeyCode::Char('s' | 'S')) {
        let ManagerStage::Settings(settings) = &mut state.stage else {
            return;
        };
        if jackin_console::services::workspace::global_rows_have_sensitive_mount(
            &settings.mounts.pending,
        ) {
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
    let content_width = settings_global_config_mounts_content_width_with_cache(
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
    let op_cache = std::rc::Rc::clone(&state.op_cache);
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
            let rows = settings.env_flat_rows();
            let is_op_ref = matches!(
                rows.get(settings.env.selected),
                Some(SettingsEnvRow::Key { scope, key })
                    if settings_update::settings_env_value(&settings.env.pending, scope, key)
                        .is_some_and(|v| matches!(v, jackin_core::EnvValue::OpRef(_)))
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

mod auth;
use auth::handle_auth_key;
pub(super) use auth::handle_settings_auth_modal;
pub(crate) use auth::settings_auth_can_generate_token;
pub(in crate::console) use auth::{
    apply_op_picker_settings_commit_failed, apply_op_picker_to_settings_auth_form_committed,
    apply_plain_text_to_settings_auth_form,
};
#[cfg(test)]
use auth::{apply_op_picker_to_settings_auth_form_with_runner, open_settings_auth_form};
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

#[expect(
    clippy::too_many_lines,
    reason = "pending extraction — tracked in codebase-readability roadmap"
)]
pub(super) fn handle_settings_confirm_modal(
    settings: &mut crate::console::tui::state::SettingsState<'_>,
    key: KeyEvent,
    term_size: ratatui::layout::Rect,
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
                if settings.mounts.modal.is_none() {
                    settings.mounts.add_draft = None;
                }
            }
            ModalOutcome::Continue => {
                settings.mounts.modal = Some(GlobalMountModal::Text { target, state });
            }
        },
        GlobalMountModal::FileBrowser { mut state } => {
            let page_rows = page_rows_for_modal(term_size, &state);
            let browser_outcome = state.handle_key_with_page_rows(key, Some(page_rows));
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
                    settings.mounts.open_sub_modal(text_modal_for_target(
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
                if settings.mounts.modal.is_none() {
                    settings.mounts.add_draft = None;
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
                if settings.mounts.modal.is_none() {
                    settings.mounts.add_draft = None;
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

#[expect(
    clippy::too_many_lines,
    reason = "pending extraction — tracked in codebase-readability roadmap"
)]
pub(super) fn handle_settings_env_modal(
    env: &mut crate::console::tui::state::SettingsEnvState<'_>,
    key: KeyEvent,
    op_cache: std::rc::Rc<std::cell::RefCell<jackin_env::OpCache>>,
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
                            crate::console::tui::op_picker::OpPickerState::new_with_cache(op_cache),
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
            ModalOutcome::Commit(crate::console::tui::op_picker::OpPickerSelection::Existing(
                op_ref,
            )) => {
                let target = env.pending_picker_target.take();
                match target {
                    Some((scope, Some(key))) => {
                        set_settings_env_value_typed(
                            env,
                            &scope,
                            &key,
                            jackin_core::EnvValue::OpRef(op_ref),
                        );
                        env.clear_modal_chain();
                    }
                    Some((scope, None)) => {
                        env.pending_picker_value = Some(jackin_core::EnvValue::OpRef(op_ref));
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
                let scope = SettingsEnvScope::Role(role_key);
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
                global.selected = settings_update::settings_global_mounts_selected_index(
                    global.selected,
                    global.pending.len(),
                );
            }
            SettingsModalOutcome::Continue
        }
        GlobalMountConfirm::Save => request_settings_save(settings),
        GlobalMountConfirm::Sensitive => {
            open_settings_save_preview(settings);
            SettingsModalOutcome::Continue
        }
        GlobalMountConfirm::Discard => {
            settings.discard_all();
            settings.mounts.exit_requested = true;
            SettingsModalOutcome::Continue
        }
    }
}

fn request_settings_save(
    settings: &mut crate::console::tui::state::SettingsState<'_>,
) -> SettingsModalOutcome {
    settings.clear_ignored_env_only_auth_keys();
    SettingsModalOutcome::SaveSettings
}

fn open_settings_save_preview(settings: &mut crate::console::tui::state::SettingsState<'_>) {
    let lines = super::save::build_settings_save_lines(settings);
    settings.mounts.modal = Some(GlobalMountModal::PreviewSave {
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
            row.mount.dst = trimmed.to_owned();
            global.clear_modal_chain();
        }
        GlobalMountTextTarget::Scope => {
            let Some(row) = global.pending.get_mut(global.selected) else {
                global.error = Some(global_mount_gone_message().into());
                return SettingsModalOutcome::Continue;
            };
            row.scope = jackin_console::services::workspace::global_mount_scope_value(trimmed);
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
            row.name = trimmed.to_owned();
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
            let key = trimmed.to_owned();
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
                jackin_core::EnvValue::Plain(value.to_owned()),
            );
            env.pending_env_key = None;
            env.clear_modal_chain();
        }
    }
}

fn open_settings_env_role_picker(env: &mut crate::console::tui::state::SettingsEnvState<'_>) {
    use crate::console::tui::state::RolePickerState;
    use jackin_core::RoleSelector;

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
    draft.scope = jackin_console::services::workspace::global_mount_scope_value(value);
    SettingsModalOutcome::OpenGlobalMountFileBrowser
}

fn commit_add_name_text(
    global: &mut crate::console::tui::state::GlobalMountsState<'_>,
    value: &str,
) {
    if value.is_empty() {
        global.error = Some(global_mount_name_empty_message().into());
        global.modal = Some(text_modal_for_target(GlobalMountTextTarget::AddName, ""));
        return;
    }
    let Some(draft) = global.add_draft.as_mut() else {
        global.error = Some(global_mount_add_draft_lost_message().into());
        return;
    };
    draft.name = value.to_owned();
    global.open_sub_modal(text_modal_for_target(GlobalMountTextTarget::AddSource, ""));
}

fn commit_add_source_text(
    global: &mut crate::console::tui::state::GlobalMountsState<'_>,
    value: &str,
) {
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
    draft.dst = value.to_owned();
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
    draft.name = jackin_console::services::workspace::unique_global_mount_name(
        &global.pending,
        draft.scope.as_deref(),
        &draft.dst,
    );
    global.pending.push(jackin_config::GlobalMountRow {
        scope: draft.scope,
        name: draft.name,
        mount: jackin_console::services::workspace::shared_mount_config(
            draft.src, draft.dst, false,
        ),
    });
    global.selected = settings_update::settings_global_mounts_added_index(global.pending.len());
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
    let rows = settings.env_flat_rows();
    let plan = settings_update::settings_env_enter_plan_for_row(
        &settings.env.pending,
        rows.get(settings.env.selected),
        |value| !value.is_some_and(|v| matches!(v, jackin_core::EnvValue::OpRef(_))),
    );
    match plan {
        SettingsEnvEnterPlan::EditValue { scope, key } => {
            let value = settings_update::settings_env_value(&settings.env.pending, &scope, &key);
            let current =
                settings_env_value_current_text(value.map(jackin_core::EnvValue::as_persisted_str));
            let target = SettingsEnvTextTarget::EnvValue {
                scope,
                key: key.clone(),
            };
            let state = settings_env_text_input_state(
                &target,
                settings_env_value_text_label(&key),
                current,
            );
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
    let rows = settings.env_flat_rows();
    let Some(scope) =
        settings_update::settings_env_add_target_for_row(rows.get(settings.env.selected))
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
    let rows = settings.env_flat_rows();
    let Some(SettingsEnvRow::Key { key, .. }) = rows.get(settings.env.selected).cloned() else {
        return;
    };
    settings.env.modal = Some(SettingsEnvModal::Confirm {
        action: SettingsEnvConfirm::Delete,
        state: settings_env_delete_confirm_state(&key),
    });
}

fn toggle_settings_env_mask(settings: &mut crate::console::tui::state::SettingsState<'_>) {
    let rows = settings.env_flat_rows();
    settings_update::toggle_settings_env_mask_for_row(
        &mut settings.env.unmasked_rows,
        &settings.env.pending,
        rows.get(settings.env.selected),
        |value| !matches!(value, jackin_core::EnvValue::OpRef(_)),
    );
}

fn open_settings_env_picker_modal(
    settings: &mut crate::console::tui::state::SettingsState<'_>,
    op_cache: std::rc::Rc<std::cell::RefCell<jackin_env::OpCache>>,
) {
    let rows = settings.env_flat_rows();
    let Some(target) =
        settings_update::settings_env_picker_target_for_row(rows.get(settings.env.selected))
    else {
        return;
    };
    settings.env.pending_picker_target = Some(target);
    settings.env.modal = Some(SettingsEnvModal::OpPicker {
        state: Box::new(crate::console::tui::op_picker::OpPickerState::new_with_cache(op_cache)),
    });
}

fn delete_selected_settings_env(env: &mut crate::console::tui::state::SettingsEnvState<'_>) {
    let rows = settings_update::settings_env_flat_rows(&env.pending, &env.expanded);
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
    value: jackin_core::EnvValue,
) {
    settings_update::set_settings_env_value(&mut env.pending, &mut env.expanded, scope, key, value);
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

fn text_modal_for_target(
    target: GlobalMountTextTarget,
    initial: &str,
) -> GlobalMountModal<'static> {
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
mod tests;
