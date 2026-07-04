#![allow(clippy::too_many_lines)]
// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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
use jackin_tui::components::KeyChord;

use crate::tui::components::auth_panel::{
    AuthFormKeyPlan, auth_credential_input_state, auth_form_key_plan_with_source_folder,
    auth_source_picker_state, generated_token_op_item_name, generated_token_source_picker_state,
};
use crate::tui::components::file_browser::page_rows_for_modal;
use crate::tui::keymap::{
    SETTINGS_CONTENT_SHELL_KEYMAP, SETTINGS_ENV_TAB_KEYMAP, SETTINGS_GENERAL_TAB_KEYMAP,
    SETTINGS_GLOBAL_MOUNTS_TAB_KEYMAP, SETTINGS_TAB_BAR_KEYMAP, SETTINGS_TRUST_TAB_KEYMAP,
    SettingsContentShellAction, SettingsEnvTabAction, SettingsGeneralTabAction,
    SettingsGlobalMountsTabAction, SettingsTabBarAction, SettingsTrustTabAction,
};
use crate::tui::mount_display::settings_global_config_mounts_content_width_with_cache;
use crate::tui::screens::settings::update as settings_update;
use crate::tui::screens::settings::update::{
    GlobalMountAddFinalizeApplyPlan, GlobalMountAddTextApplyPlan, GlobalMountEditTextApplyPlan,
    GlobalMountGithubOpenPlan, GlobalMountRolePickerCommitPlan, GlobalMountScopePickerCommitPlan,
    GlobalMountTextCommitPlan, RolePickerOpenPlan, SettingsAuthKeyPlan, SettingsEnvHeaderKeyPlan,
    SettingsEnvOpPickerCommitPlan, SettingsEnvScopePickerCommitPlan,
    SettingsEnvScopePickerSelection, SettingsEnvSourcePickerCommitPlan,
    SettingsEnvSourcePickerSelection, SettingsEnvTextCommitPlan,
};
use crate::tui::screens::settings::view::{
    global_mount_add_draft_lost_message, global_mount_confirm_state,
    global_mount_destination_empty_message, global_mount_gone_message,
    global_mount_name_empty_message, global_mount_no_github_url_message,
    global_mount_scope_picker_state, global_mount_selected_edit_text_plan,
    global_mount_text_input_state, global_mount_text_target_label,
    settings_env_delete_confirm_state, settings_env_empty_key_error_message,
    settings_env_empty_key_text_plan, settings_env_key_input_state,
    settings_env_new_key_after_picker_text_plan, settings_env_new_key_text_plan,
    settings_env_plain_value_text_plan, settings_env_scope_picker_state,
    settings_env_source_picker_state, settings_env_text_input_state,
    settings_env_value_edit_text_plan, settings_error_popup_title,
    settings_no_registered_roles_error_message, settings_sensitive_paths_not_confirmed_message,
};
use crate::tui::state::ManagerEffect;
use crate::tui::state::update::{ManagerMessage, update_manager};
use crate::tui::state::{
    AuthForm, AuthFormFocus, AuthFormTarget, GlobalMountConfirm, GlobalMountModal,
    GlobalMountTextTarget, ManagerStage, ManagerState, RolePickerState, SettingsAuthModal,
    SettingsEnvConfirm, SettingsEnvEnterPlan, SettingsEnvModal, SettingsEnvScope,
    SettingsEnvTextTarget, SettingsTab,
};
use crate::tui::update::{
    BoolConfirmModalPlan, ConfirmSaveModalPlan, FileBrowserModalPlan, InlinePickerPlan,
    MountDstChoicePlan, ScopePickerPlan, SourcePickerPlan, bool_confirm_modal_plan,
    confirm_save_modal_plan, file_browser_modal_plan, inline_picker_plan, mount_dst_choice_plan,
    scope_picker_plan, source_picker_plan,
};

pub type SettingsModalOutcome = crate::tui::message::ConsoleSettingsModalOutcome;

pub type SettingsAuthOutcome = crate::tui::message::ConsoleSettingsAuthOutcome<jackin_core::OpRef>;

#[cfg(test)]
pub fn handle_settings_key(state: &mut ManagerState<'_>, key: KeyEvent) {
    handle_settings_key_with_effects(state, key);
}

pub fn handle_settings_key_with_effects(state: &mut ManagerState<'_>, key: KeyEvent) {
    let ManagerStage::Settings(settings) = &state.stage else {
        return;
    };

    let chord = KeyChord::from(key);
    let tab_bar_focused = settings.tab_bar_focused();
    let auth_kind_selected = settings.auth.has_selected_kind();
    let active_tab = settings.active_tab;

    // Shell: tab-bar navigation takes priority over per-tab dispatch.
    if tab_bar_focused {
        match SETTINGS_TAB_BAR_KEYMAP.dispatch(chord) {
            Some(SettingsTabBarAction::PrevTab) => {
                dispatch_manager(
                    state,
                    ManagerMessage::MoveSettingsTab {
                        delta: -1,
                        focus_tab_bar: true,
                    },
                );
                return;
            }
            Some(SettingsTabBarAction::NextTab) => {
                dispatch_manager(
                    state,
                    ManagerMessage::MoveSettingsTab {
                        delta: 1,
                        focus_tab_bar: true,
                    },
                );
                return;
            }
            Some(SettingsTabBarAction::FocusContent) => {
                dispatch_manager(state, ManagerMessage::FocusSettingsContent);
                return;
            }
            None => {}
        }
    } else {
        // Content mode: shell intercepts Tab, BackTab, Esc before per-tab.
        match SETTINGS_CONTENT_SHELL_KEYMAP.dispatch(chord) {
            Some(SettingsContentShellAction::NextTab) => {
                dispatch_manager(
                    state,
                    ManagerMessage::MoveSettingsTab {
                        delta: 1,
                        focus_tab_bar: true,
                    },
                );
                return;
            }
            Some(SettingsContentShellAction::FocusTabBar) => {
                dispatch_manager(state, ManagerMessage::FocusSettingsTabBar);
                return;
            }
            Some(SettingsContentShellAction::FocusTabBarOrClearAuth) => {
                if auth_kind_selected {
                    dispatch_manager(state, ManagerMessage::ClearSettingsAuthKind);
                }
                dispatch_manager(state, ManagerMessage::FocusSettingsTabBar);
                return;
            }
            None => {}
        }
    }

    // Env role header: Left/Right expand/collapse a role row in the env tab.
    let ManagerStage::Settings(settings) = &state.stage else {
        return;
    };
    match settings_update::settings_env_selected_header_key_plan(
        key.code,
        active_tab,
        &settings.env.pending,
        &settings.env.expanded,
        settings.env.selected,
    ) {
        SettingsEnvHeaderKeyPlan::SetExpanded { role, expanded } => {
            dispatch_manager(
                state,
                ManagerMessage::SetSettingsEnvRoleExpanded { role, expanded },
            );
            return;
        }
        SettingsEnvHeaderKeyPlan::Consume => return,
        SettingsEnvHeaderKeyPlan::Continue => {}
    }

    // Per-tab dispatch.
    match active_tab {
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

fn handle_global_mounts_key(state: &mut ManagerState<'_>, key: KeyEvent) {
    let ManagerStage::Settings(settings) = &state.stage else {
        return;
    };
    let is_dirty = settings.is_dirty();
    let has_sensitive_mount =
        crate::services::workspace::global_rows_have_sensitive_mount(&settings.mounts.pending);
    let selected = settings.mounts.selected;
    let mount_count = settings.mounts.pending.len();
    let term_width = state.cached_term_size.width;
    let content_width = settings_global_config_mounts_content_width_with_cache(
        &settings.mounts.pending,
        &settings.mounts.mount_info_cache,
    );
    let footer_h = settings.cached_footer_h;
    let chord = KeyChord::from(key);
    match SETTINGS_GLOBAL_MOUNTS_TAB_KEYMAP.dispatch(chord) {
        Some(SettingsGlobalMountsTabAction::Save) => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                return;
            };
            if has_sensitive_mount {
                settings.mounts.modal = Some(confirm_modal(GlobalMountConfirm::Sensitive));
            } else {
                open_settings_save_preview(settings);
            }
        }
        Some(SettingsGlobalMountsTabAction::ScrollLeft) => {
            dispatch_manager(
                state,
                ManagerMessage::ScrollSettingsGlobalMountsHorizontal {
                    delta: -8,
                    term_width,
                    content_width,
                },
            );
        }
        Some(SettingsGlobalMountsTabAction::ScrollRight) => {
            dispatch_manager(
                state,
                ManagerMessage::ScrollSettingsGlobalMountsHorizontal {
                    delta: 8,
                    term_width,
                    content_width,
                },
            );
        }
        Some(SettingsGlobalMountsTabAction::MoveUp) => {
            dispatch_manager(
                state,
                ManagerMessage::MoveSettingsGlobalMountsSelection {
                    delta: -1,
                    term: state.cached_term_size,
                    footer_h,
                },
            );
        }
        Some(SettingsGlobalMountsTabAction::MoveDown) => {
            dispatch_manager(
                state,
                ManagerMessage::MoveSettingsGlobalMountsSelection {
                    delta: 1,
                    term: state.cached_term_size,
                    footer_h,
                },
            );
        }
        Some(SettingsGlobalMountsTabAction::ToggleReadonly) => {
            dispatch_manager(state, ManagerMessage::ToggleSettingsGlobalMountReadonly);
        }
        Some(SettingsGlobalMountsTabAction::Back) => {
            if is_dirty {
                let ManagerStage::Settings(settings) = &mut state.stage else {
                    return;
                };
                settings.mounts.modal = Some(confirm_modal(GlobalMountConfirm::Discard));
            } else {
                dispatch_manager(state, ManagerMessage::ReturnToList);
            }
        }
        Some(SettingsGlobalMountsTabAction::Add) => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                return;
            };
            open_global_mount_scope_picker(&mut settings.mounts);
        }
        Some(SettingsGlobalMountsTabAction::Enter) => {
            if settings_update::settings_global_mounts_add_row_selected(selected, mount_count) {
                let ManagerStage::Settings(settings) = &mut state.stage else {
                    return;
                };
                open_global_mount_scope_picker(&mut settings.mounts);
            }
        }
        Some(SettingsGlobalMountsTabAction::Delete) if mount_count > 0 => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                return;
            };
            settings.mounts.modal = Some(confirm_modal(GlobalMountConfirm::Remove));
        }
        Some(SettingsGlobalMountsTabAction::OpenGithub) => {
            let plan = {
                let ManagerStage::Settings(settings) = &mut state.stage else {
                    return;
                };
                let global = &mut settings.mounts;
                settings_update::global_mount_github_open_plan(
                    &global.pending,
                    global.selected,
                    &global.mount_info_cache,
                )
            };
            match plan {
                GlobalMountGithubOpenPlan::NoSelection => {}
                GlobalMountGithubOpenPlan::NoGithubUrl => {
                    let ManagerStage::Settings(settings) = &mut state.stage else {
                        return;
                    };
                    settings
                        .mounts
                        .set_error(global_mount_no_github_url_message());
                }
                GlobalMountGithubOpenPlan::Open(web_url) => {
                    state.request_effect(ManagerEffect::OpenUrl(web_url));
                }
            }
        }
        Some(SettingsGlobalMountsTabAction::EditRename) => {
            open_edit_text(state, GlobalMountTextTarget::Rename);
        }
        Some(SettingsGlobalMountsTabAction::EditSource) => {
            open_edit_text(state, GlobalMountTextTarget::Source);
        }
        Some(SettingsGlobalMountsTabAction::EditDest) => {
            open_edit_text(state, GlobalMountTextTarget::Destination);
        }
        Some(SettingsGlobalMountsTabAction::EditScope) => {
            open_edit_text(state, GlobalMountTextTarget::Scope);
        }
        // Context check failed (Delete with mount_count == 0) or no binding.
        Some(_) | None => {}
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
    let is_dirty = settings.is_dirty();
    let plain_modifier = (key.modifiers - KeyModifiers::SHIFT).is_empty();
    let selected_is_op_ref = settings_update::settings_env_selected_is_op_ref(
        &settings.env.pending,
        &settings.env.expanded,
        settings.env.selected,
    );
    let chord = KeyChord::from(key);
    match SETTINGS_ENV_TAB_KEYMAP.dispatch(chord) {
        Some(SettingsEnvTabAction::MoveUp) => {
            dispatch_manager(
                state,
                ManagerMessage::MoveSettingsEnvSelection {
                    delta: -1,
                    term: term_size,
                    footer_h,
                },
            );
        }
        Some(SettingsEnvTabAction::MoveDown) => {
            dispatch_manager(
                state,
                ManagerMessage::MoveSettingsEnvSelection {
                    delta: 1,
                    term: term_size,
                    footer_h,
                },
            );
        }
        Some(SettingsEnvTabAction::Add) => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                return;
            };
            open_settings_env_add_modal(settings);
        }
        Some(SettingsEnvTabAction::Save) => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                return;
            };
            open_settings_save_preview(settings);
        }
        Some(SettingsEnvTabAction::Delete) if plain_modifier => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                return;
            };
            open_settings_env_delete_confirm(settings);
        }
        Some(SettingsEnvTabAction::ToggleMask) if plain_modifier => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                return;
            };
            toggle_settings_env_mask(settings);
        }
        Some(SettingsEnvTabAction::OpenPicker) if plain_modifier && op_available => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                return;
            };
            open_settings_env_picker_modal(settings, op_cache);
        }
        Some(SettingsEnvTabAction::Enter) => {
            if selected_is_op_ref && op_available {
                let ManagerStage::Settings(settings) = &mut state.stage else {
                    return;
                };
                open_settings_env_picker_modal(settings, op_cache);
            } else {
                let ManagerStage::Settings(settings) = &mut state.stage else {
                    return;
                };
                open_settings_env_enter_modal(settings);
            }
        }
        Some(SettingsEnvTabAction::Back) => {
            if is_dirty {
                let ManagerStage::Settings(settings) = &mut state.stage else {
                    return;
                };
                settings.mounts.modal = Some(confirm_modal(GlobalMountConfirm::Discard));
            } else {
                dispatch_manager(state, ManagerMessage::ReturnToList);
            }
        }
        // Context check failed (Delete/ToggleMask/OpenPicker without plain_modifier) or no binding.
        Some(_) | None => {}
    }
}

mod auth;
pub(crate) use auth::apply_source_folder_to_settings_auth_form;
use auth::handle_auth_key;
pub use auth::handle_settings_auth_modal;
pub use auth::settings_auth_can_generate_token;
pub use auth::{
    apply_op_picker_to_settings_auth_form_committed, apply_plain_text_to_settings_auth_form,
};
#[cfg(test)]
use auth::{apply_op_picker_to_settings_auth_form_with_runner, open_settings_auth_form};
fn handle_general_key(state: &mut ManagerState<'_>, key: KeyEvent) {
    let ManagerStage::Settings(settings) = &state.stage else {
        return;
    };
    let is_dirty = settings.is_dirty();
    let chord = KeyChord::from(key);
    match SETTINGS_GENERAL_TAB_KEYMAP.dispatch(chord) {
        Some(SettingsGeneralTabAction::MoveUp) => {
            dispatch_manager(
                state,
                ManagerMessage::MoveSettingsGeneralSelection { delta: -1 },
            );
        }
        Some(SettingsGeneralTabAction::MoveDown) => {
            dispatch_manager(
                state,
                ManagerMessage::MoveSettingsGeneralSelection { delta: 1 },
            );
        }
        Some(SettingsGeneralTabAction::Toggle) => {
            dispatch_manager(state, ManagerMessage::ToggleSettingsGeneralSelected);
        }
        Some(SettingsGeneralTabAction::Save) => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                return;
            };
            open_settings_save_preview(settings);
        }
        Some(SettingsGeneralTabAction::Back) => {
            if is_dirty {
                let ManagerStage::Settings(settings) = &mut state.stage else {
                    return;
                };
                settings.mounts.modal = Some(confirm_modal(GlobalMountConfirm::Discard));
            } else {
                dispatch_manager(state, ManagerMessage::ReturnToList);
            }
        }
        None => {}
    }
}

fn handle_trust_key(state: &mut ManagerState<'_>, key: KeyEvent) {
    let term_size = state.cached_term_size;
    let term_width = term_size.width;
    let ManagerStage::Settings(settings) = &state.stage else {
        return;
    };
    let footer_h = settings.cached_footer_h;
    let is_dirty = settings.is_dirty();
    let content_width = settings_update::trust_content_width(&settings.trust);
    let chord = KeyChord::from(key);
    match SETTINGS_TRUST_TAB_KEYMAP.dispatch(chord) {
        Some(SettingsTrustTabAction::MoveUp) => {
            dispatch_manager(
                state,
                ManagerMessage::MoveSettingsTrustSelection {
                    delta: -1,
                    term: term_size,
                    footer_h,
                },
            );
        }
        Some(SettingsTrustTabAction::MoveDown) => {
            dispatch_manager(
                state,
                ManagerMessage::MoveSettingsTrustSelection {
                    delta: 1,
                    term: term_size,
                    footer_h,
                },
            );
        }
        Some(SettingsTrustTabAction::ScrollLeft) => {
            dispatch_manager(
                state,
                ManagerMessage::ScrollSettingsTrustHorizontal {
                    delta: -8,
                    term_width,
                    content_width,
                },
            );
        }
        Some(SettingsTrustTabAction::ScrollRight) => {
            dispatch_manager(
                state,
                ManagerMessage::ScrollSettingsTrustHorizontal {
                    delta: 8,
                    term_width,
                    content_width,
                },
            );
        }
        Some(SettingsTrustTabAction::Toggle) => {
            dispatch_manager(state, ManagerMessage::ToggleSettingsTrustSelected);
        }
        Some(SettingsTrustTabAction::Save) => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                return;
            };
            open_settings_save_preview(settings);
        }
        Some(SettingsTrustTabAction::Back) => {
            if is_dirty {
                let ManagerStage::Settings(settings) = &mut state.stage else {
                    return;
                };
                settings.mounts.modal = Some(confirm_modal(GlobalMountConfirm::Discard));
            } else {
                dispatch_manager(state, ManagerMessage::ReturnToList);
            }
        }
        None => {}
    }
}

pub fn handle_settings_confirm_modal(
    settings: &mut crate::tui::state::SettingsState<'_>,
    key: KeyEvent,
    term_size: ratatui::layout::Rect,
) -> SettingsModalOutcome {
    let Some(modal) = settings.mounts.modal.take() else {
        return SettingsModalOutcome::Continue;
    };
    let mut outcome = SettingsModalOutcome::Continue;
    match modal {
        GlobalMountModal::Text { target, mut state } => {
            match inline_picker_plan(state.handle_key(key)) {
                InlinePickerPlan::Commit(value) => {
                    let committed_target = target.clone();
                    settings.mounts.modal = Some(GlobalMountModal::Text { target, state });
                    outcome = commit_text(&mut settings.mounts, &committed_target, &value);
                }
                InlinePickerPlan::Dismiss => {
                    settings
                        .mounts
                        .pop_modal_chain_and_clear_add_draft_if_closed();
                }
                InlinePickerPlan::Continue => {
                    settings.mounts.modal = Some(GlobalMountModal::Text { target, state });
                }
            }
        }
        GlobalMountModal::FileBrowser { mut state } => {
            let page_rows = page_rows_for_modal(term_size, &state);
            let browser_outcome = state.handle_key_with_page_rows(key, Some(page_rows));
            match file_browser_modal_plan(browser_outcome) {
                FileBrowserModalPlan::Dismiss => {
                    settings
                        .mounts
                        .pop_modal_chain_and_clear_add_draft_if_closed();
                }
                FileBrowserModalPlan::ResolveGitUrl(path) => {
                    settings.mounts.modal = Some(GlobalMountModal::FileBrowser { state });
                    outcome = SettingsModalOutcome::ResolveFileBrowserGitUrl(path);
                }
                FileBrowserModalPlan::OpenUrl(url) => {
                    settings.mounts.modal = Some(GlobalMountModal::FileBrowser { state });
                    outcome = SettingsModalOutcome::OpenUrl(url);
                }
                FileBrowserModalPlan::Continue => {
                    settings.mounts.modal = Some(GlobalMountModal::FileBrowser { state });
                }
                FileBrowserModalPlan::ApplyFileBrowserOutcome(browser_outcome) => {
                    settings.mounts.modal = Some(GlobalMountModal::FileBrowser { state });
                    outcome = SettingsModalOutcome::ApplyFileBrowserOutcome(browser_outcome);
                }
            }
        }
        GlobalMountModal::MountDstChoice { mut state } => {
            let src = state.src.clone();
            match mount_dst_choice_plan(state.handle_key(key)) {
                MountDstChoicePlan::CommitSamePath => {
                    settings_update::set_global_mount_add_draft_destination(
                        &mut settings.mounts.add_draft,
                        src,
                    );
                    finalize_global_mount_add(&mut settings.mounts);
                }
                MountDstChoicePlan::OpenEditInput => {
                    settings_update::set_global_mount_add_draft_destination(
                        &mut settings.mounts.add_draft,
                        src.clone(),
                    );
                    settings.mounts.modal = Some(GlobalMountModal::MountDstChoice { state });
                    settings.mounts.open_sub_modal(text_modal_for_target(
                        GlobalMountTextTarget::AddDestination,
                        &src,
                    ));
                }
                MountDstChoicePlan::Dismiss => {
                    settings
                        .mounts
                        .pop_modal_chain_and_clear_add_draft_if_closed();
                }
                MountDstChoicePlan::Continue => {
                    settings.mounts.modal = Some(GlobalMountModal::MountDstChoice { state });
                }
            }
        }
        GlobalMountModal::ScopePicker { mut state } => {
            match scope_picker_plan(state.handle_key(key)) {
                ScopePickerPlan::AllAgents | ScopePickerPlan::SpecificAgent => {
                    // Drop the picker before dispatching: commit_text
                    // (AllAgents path) calls clear_modal_chain anyway, and
                    // open_sub_modal (SpecificAgent → RolePicker) would
                    // otherwise stash this already-committed picker as
                    // the RolePicker's parent — Esc on RolePicker would
                    // then resurrect a consumed ScopePicker.
                    let choice = state.focused;
                    outcome = commit_add_scope_choice(settings, choice);
                }
                ScopePickerPlan::Dismiss => {
                    settings
                        .mounts
                        .pop_modal_chain_and_clear_add_draft_if_closed();
                }
                ScopePickerPlan::Continue => {
                    settings.mounts.modal = Some(GlobalMountModal::ScopePicker { state });
                }
            }
        }
        GlobalMountModal::RolePicker { state: mut picker } => {
            match inline_picker_plan(picker.handle_key(key)) {
                InlinePickerPlan::Commit(role) => {
                    match settings_update::global_mount_role_picker_commit_plan(
                        &mut settings.mounts.add_draft,
                        &role,
                    ) {
                        GlobalMountRolePickerCommitPlan::OpenFileBrowser => {
                            settings.mounts.modal =
                                Some(GlobalMountModal::RolePicker { state: picker });
                            outcome = SettingsModalOutcome::OpenGlobalMountFileBrowser;
                        }
                        GlobalMountRolePickerCommitPlan::MissingDraft => {
                            settings
                                .mounts
                                .set_error(global_mount_add_draft_lost_message());
                        }
                    }
                }
                InlinePickerPlan::Dismiss => {
                    settings
                        .mounts
                        .pop_modal_chain_and_clear_add_draft_if_closed();
                }
                InlinePickerPlan::Continue => {
                    settings.mounts.modal = Some(GlobalMountModal::RolePicker { state: picker });
                }
            }
        }
        GlobalMountModal::Confirm { action, mut state } => {
            match settings_update::settings_confirm_plan(action, state.handle_key(key)) {
                settings_update::SettingsConfirmPlan::Commit => {
                    outcome = commit_settings_confirm(settings, action);
                }
                settings_update::SettingsConfirmPlan::Cancel { abort_sensitive } => {
                    if abort_sensitive {
                        settings
                            .mounts
                            .set_error(settings_sensitive_paths_not_confirmed_message());
                    }
                    settings.mounts.clear_modal_chain();
                }
                settings_update::SettingsConfirmPlan::Continue => {
                    settings.mounts.modal = Some(GlobalMountModal::Confirm { action, state });
                }
            }
        }
        GlobalMountModal::PreviewSave { mut state } => {
            match confirm_save_modal_plan(state.handle_key(key)) {
                ConfirmSaveModalPlan::Commit => {
                    outcome = request_settings_save(settings);
                }
                ConfirmSaveModalPlan::Dismiss => settings.mounts.clear_modal_chain(),
                ConfirmSaveModalPlan::Continue => {
                    settings.mounts.modal = Some(GlobalMountModal::PreviewSave { state });
                }
            }
        }
    }
    outcome
}

pub fn handle_settings_env_modal(
    env: &mut crate::tui::state::SettingsEnvState<'_>,
    key: KeyEvent,
    op_cache: std::rc::Rc<std::cell::RefCell<jackin_env::OpCache>>,
) {
    let Some(modal) = env.modal.take() else {
        return;
    };
    match modal {
        SettingsEnvModal::Text { target, mut state } => {
            match inline_picker_plan(state.handle_key(key)) {
                InlinePickerPlan::Commit(value) => {
                    let committed_target = target.clone();
                    env.modal = Some(SettingsEnvModal::Text { target, state });
                    commit_env_text(env, &committed_target, &value);
                }
                InlinePickerPlan::Dismiss => {
                    env.pop_modal_chain_and_clear_pending_env_key_if_closed();
                }
                InlinePickerPlan::Continue => {
                    env.modal = Some(SettingsEnvModal::Text { target, state });
                }
            }
        }
        SettingsEnvModal::SourcePicker { state: mut source } => {
            match source_picker_plan(source.handle_key(key)) {
                SourcePickerPlan::Plain => {
                    commit_settings_env_source_picker(
                        env,
                        SettingsEnvSourcePickerSelection::Plain,
                        source,
                        op_cache,
                    );
                }
                SourcePickerPlan::Op => {
                    commit_settings_env_source_picker(
                        env,
                        SettingsEnvSourcePickerSelection::Op,
                        source,
                        op_cache,
                    );
                }
                SourcePickerPlan::Dismiss => {
                    env.pop_modal_chain_and_clear_pending_env_key();
                }
                SourcePickerPlan::Continue => {
                    env.modal = Some(SettingsEnvModal::SourcePicker { state: source });
                }
            }
        }
        SettingsEnvModal::OpPicker { state: mut picker } => {
            match inline_picker_plan(picker.handle_key(key)) {
                // Browse-mode caller: only `Existing` is reachable.
                InlinePickerPlan::Commit(
                    crate::tui::op_picker::OpPickerSelection::NewItem { .. }
                    | crate::tui::op_picker::OpPickerSelection::EditItemField { .. },
                ) => unreachable!("settings-env OpPicker runs in Browse mode"),
                InlinePickerPlan::Commit(crate::tui::op_picker::OpPickerSelection::Existing(
                    op_ref,
                )) => {
                    let plan = settings_update::settings_env_op_picker_commit_plan(
                        env.pending_picker_target.as_ref(),
                    );
                    env.clear_pending_picker_target();
                    match plan {
                        SettingsEnvOpPickerCommitPlan::SetExisting { scope, key } => {
                            set_settings_env_value_typed(
                                env,
                                &scope,
                                &key,
                                jackin_core::EnvValue::OpRef(op_ref),
                            );
                            env.clear_modal_chain();
                        }
                        SettingsEnvOpPickerCommitPlan::StashForNewKey { scope } => {
                            env.stash_pending_picker_value(jackin_core::EnvValue::OpRef(op_ref));
                            let plan = settings_env_new_key_after_picker_text_plan(scope);
                            let state = settings_env_key_input_state(
                                &env.pending,
                                &plan.scope,
                                plan.label,
                                "",
                            );
                            env.modal = Some(SettingsEnvModal::OpPicker { state: picker });
                            env.open_sub_modal(SettingsEnvModal::Text {
                                target: plan.target,
                                state: Box::new(state),
                            });
                        }
                        SettingsEnvOpPickerCommitPlan::MissingTarget => env.clear_modal_chain(),
                    }
                }
                InlinePickerPlan::Dismiss => {
                    env.pop_modal_chain_and_clear_picker_target();
                }
                InlinePickerPlan::Continue => {
                    env.modal = Some(SettingsEnvModal::OpPicker { state: picker });
                }
            }
        }
        SettingsEnvModal::RolePicker { state: mut picker } => {
            match inline_picker_plan(picker.handle_key(key)) {
                InlinePickerPlan::Commit(role) => {
                    let plan = settings_update::settings_env_role_picker_commit_plan(&role);
                    let text_plan = settings_env_new_key_text_plan(plan.scope);
                    let state = settings_env_key_input_state(
                        &env.pending,
                        &text_plan.scope,
                        text_plan.label,
                        "",
                    );
                    env.modal = Some(SettingsEnvModal::RolePicker { state: picker });
                    env.open_sub_modal(SettingsEnvModal::Text {
                        target: text_plan.target,
                        state: Box::new(state),
                    });
                }
                InlinePickerPlan::Dismiss => {
                    env.pop_modal_chain();
                }
                InlinePickerPlan::Continue => {
                    env.modal = Some(SettingsEnvModal::RolePicker { state: picker });
                }
            }
        }
        SettingsEnvModal::ScopePicker { mut state } => {
            match scope_picker_plan(state.handle_key(key)) {
                ScopePickerPlan::AllAgents => {
                    commit_settings_env_scope_picker(
                        env,
                        SettingsEnvScopePickerSelection::AllAgents,
                    );
                }
                ScopePickerPlan::SpecificAgent => {
                    commit_settings_env_scope_picker(
                        env,
                        SettingsEnvScopePickerSelection::SpecificAgent,
                    );
                }
                ScopePickerPlan::Dismiss => {
                    env.pop_modal_chain();
                }
                ScopePickerPlan::Continue => {
                    env.modal = Some(SettingsEnvModal::ScopePicker { state });
                }
            }
        }
        SettingsEnvModal::Confirm { action, mut state } => {
            match bool_confirm_modal_plan(state.handle_key(key)) {
                BoolConfirmModalPlan::Confirm => match action {
                    SettingsEnvConfirm::Delete => {
                        delete_selected_settings_env(env);
                        env.clear_modal_chain();
                    }
                },
                BoolConfirmModalPlan::Dismiss => env.clear_modal_chain(),
                BoolConfirmModalPlan::Continue => {
                    env.modal = Some(SettingsEnvModal::Confirm { action, state });
                }
            }
        }
    }
}

fn commit_settings_confirm(
    settings: &mut crate::tui::state::SettingsState<'_>,
    action: GlobalMountConfirm,
) -> SettingsModalOutcome {
    let plan = settings_update::settings_confirm_commit_plan(
        action,
        settings.mounts.selected,
        settings.mounts.pending.len(),
    );
    match plan {
        settings_update::SettingsConfirmCommitPlan::Remove {
            remove_index,
            selected,
        } => {
            settings
                .mounts
                .remove_row_and_select(remove_index, selected);
            SettingsModalOutcome::Continue
        }
        settings_update::SettingsConfirmCommitPlan::Save => request_settings_save(settings),
        settings_update::SettingsConfirmCommitPlan::OpenSavePreview => {
            open_settings_save_preview(settings);
            SettingsModalOutcome::Continue
        }
        settings_update::SettingsConfirmCommitPlan::DiscardAll => {
            settings.discard_all();
            settings.mounts.request_exit();
            SettingsModalOutcome::Continue
        }
        settings_update::SettingsConfirmCommitPlan::Noop => SettingsModalOutcome::Continue,
    }
}

fn request_settings_save(
    settings: &mut crate::tui::state::SettingsState<'_>,
) -> SettingsModalOutcome {
    settings.clear_ignored_env_only_auth_keys();
    SettingsModalOutcome::SaveSettings
}

fn open_settings_save_preview(settings: &mut crate::tui::state::SettingsState<'_>) {
    let lines = super::save::build_settings_save_lines(settings);
    settings.mounts.modal = Some(GlobalMountModal::PreviewSave {
        state: crate::tui::components::confirm_save::ConfirmSaveState::new(lines),
    });
}

fn commit_text(
    global: &mut crate::tui::state::GlobalMountsState<'_>,
    target: &GlobalMountTextTarget,
    value: &str,
) -> SettingsModalOutcome {
    match settings_update::global_mount_text_commit_plan(target, value) {
        plan @ (GlobalMountTextCommitPlan::AddScope(_)
        | GlobalMountTextCommitPlan::AddName(_)
        | GlobalMountTextCommitPlan::AddSource(_)
        | GlobalMountTextCommitPlan::AddDestination(_)) => {
            return apply_global_mount_add_text(global, plan);
        }
        plan => match settings_update::global_mount_edit_text_apply_plan(
            &mut global.pending,
            global.selected,
            plan,
        ) {
            GlobalMountEditTextApplyPlan::MissingRow => {
                global.set_error(global_mount_gone_message());
                return SettingsModalOutcome::Continue;
            }
            GlobalMountEditTextApplyPlan::EmptyName => {
                global.set_error(global_mount_name_empty_message());
                return SettingsModalOutcome::Continue;
            }
            GlobalMountEditTextApplyPlan::Applied => {
                global.clear_modal_chain();
            }
            GlobalMountEditTextApplyPlan::Noop => {}
        },
    }
    SettingsModalOutcome::Continue
}

fn commit_env_text(
    env: &mut crate::tui::state::SettingsEnvState<'_>,
    target: &SettingsEnvTextTarget,
    value: &str,
) {
    match settings_update::settings_env_text_commit_plan(
        target,
        value,
        env.has_pending_picker_value(),
    ) {
        SettingsEnvTextCommitPlan::EmptyKey { scope } => {
            env.set_error(settings_env_empty_key_error_message());
            let plan = settings_env_empty_key_text_plan(scope);
            let state = settings_env_key_input_state(&env.pending, &plan.scope, plan.label, "");
            env.modal = Some(SettingsEnvModal::Text {
                target: plan.target,
                state: Box::new(state),
            });
        }
        SettingsEnvTextCommitPlan::SetPendingPickerValue { scope, key } => {
            if let Some(stashed) = env.take_pending_picker_value() {
                set_settings_env_value_typed(env, &scope, &key, stashed);
                env.clear_pending_env_key();
                env.clear_modal_chain();
            }
        }
        SettingsEnvTextCommitPlan::OpenSourcePicker { scope, key } => {
            env.set_pending_env_key(scope, key.clone());
            env.open_sub_modal(SettingsEnvModal::SourcePicker {
                state: settings_env_source_picker_state(key),
            });
        }
        SettingsEnvTextCommitPlan::SetPlainValue { scope, key, value } => {
            set_settings_env_value_typed(env, &scope, &key, jackin_core::EnvValue::Plain(value));
            env.clear_pending_env_key();
            env.clear_modal_chain();
        }
    }
}

fn commit_settings_env_source_picker(
    env: &mut crate::tui::state::SettingsEnvState<'_>,
    selection: SettingsEnvSourcePickerSelection,
    source: crate::tui::components::source_picker::SourcePickerState,
    op_cache: std::rc::Rc<std::cell::RefCell<jackin_env::OpCache>>,
) {
    match settings_update::settings_env_source_picker_commit_plan(
        selection,
        env.pending_env_key.as_ref(),
    ) {
        SettingsEnvSourcePickerCommitPlan::MissingPendingKey => {
            env.clear_modal_chain();
        }
        SettingsEnvSourcePickerCommitPlan::OpenPlainText { scope, key } => {
            env.modal = Some(SettingsEnvModal::SourcePicker { state: source });
            let plan = settings_env_plain_value_text_plan(scope, key);
            env.open_sub_modal(env_text_modal(plan.target, plan.label, plan.current));
        }
        SettingsEnvSourcePickerCommitPlan::OpenOpPicker { scope, key } => {
            env.set_pending_picker_target((scope, Some(key)));
            env.clear_pending_env_key();
            env.modal = Some(SettingsEnvModal::SourcePicker { state: source });
            env.open_sub_modal(SettingsEnvModal::OpPicker {
                state: Box::new(crate::tui::op_picker::OpPickerState::new_with_cache(
                    op_cache,
                )),
            });
        }
    }
}

fn commit_settings_env_scope_picker(
    env: &mut crate::tui::state::SettingsEnvState<'_>,
    selection: SettingsEnvScopePickerSelection,
) {
    match settings_update::settings_env_scope_picker_commit_plan(selection) {
        SettingsEnvScopePickerCommitPlan::OpenGlobalKeyInput { scope } => {
            let plan = settings_env_new_key_text_plan(scope);
            let input_state =
                settings_env_key_input_state(&env.pending, &plan.scope, plan.label, "");
            // Don't stash the just-committed ScopePicker as
            // the Text modal's parent — Esc on Text would
            // pop back into a consumed picker. Start the
            // child modal with an empty parent chain.
            env.open_sub_modal(SettingsEnvModal::Text {
                target: plan.target,
                state: Box::new(input_state),
            });
        }
        SettingsEnvScopePickerCommitPlan::OpenRolePicker => {
            open_settings_env_role_picker(env);
        }
    }
}

fn open_settings_env_role_picker(env: &mut crate::tui::state::SettingsEnvState<'_>) {
    use crate::tui::state::RolePickerState;

    match settings_update::settings_env_role_picker_open_plan(&env.pending) {
        RolePickerOpenPlan::NoRoles => {
            env.set_error(settings_no_registered_roles_error_message());
        }
        RolePickerOpenPlan::Open(roles) => {
            env.open_sub_modal(SettingsEnvModal::RolePicker {
                state: RolePickerState::new(roles),
            });
        }
    }
}

fn apply_global_mount_add_text(
    global: &mut crate::tui::state::GlobalMountsState<'_>,
    plan: GlobalMountTextCommitPlan,
) -> SettingsModalOutcome {
    match settings_update::global_mount_add_text_apply_plan(&mut global.add_draft, plan) {
        GlobalMountAddTextApplyPlan::MissingDraft => {
            global.set_error(global_mount_add_draft_lost_message());
            SettingsModalOutcome::Continue
        }
        GlobalMountAddTextApplyPlan::OpenFileBrowser => {
            SettingsModalOutcome::OpenGlobalMountFileBrowser
        }
        GlobalMountAddTextApplyPlan::OpenAddSource => {
            global.open_sub_modal(text_modal_for_target(GlobalMountTextTarget::AddSource, ""));
            SettingsModalOutcome::Continue
        }
        GlobalMountAddTextApplyPlan::OpenAddDestination => {
            global.open_sub_modal(text_modal_for_target(
                GlobalMountTextTarget::AddDestination,
                "",
            ));
            SettingsModalOutcome::Continue
        }
        GlobalMountAddTextApplyPlan::Finalize => {
            finalize_global_mount_add(global);
            SettingsModalOutcome::Continue
        }
        GlobalMountAddTextApplyPlan::Noop => SettingsModalOutcome::Continue,
    }
}

fn open_global_mount_scope_picker(global: &mut crate::tui::state::GlobalMountsState<'_>) {
    global.start_add_draft();
    global.modal = Some(scope_picker_modal());
}

fn finalize_global_mount_add(global: &mut crate::tui::state::GlobalMountsState<'_>) {
    match settings_update::global_mount_add_finalize_apply_plan(
        &global.pending,
        &mut global.add_draft,
    ) {
        GlobalMountAddFinalizeApplyPlan::MissingDraft => {
            global.set_error(global_mount_add_draft_lost_message());
        }
        GlobalMountAddFinalizeApplyPlan::EmptyDestination => {
            global.set_error(global_mount_destination_empty_message());
        }
        GlobalMountAddFinalizeApplyPlan::Add { row, selected } => {
            global.add_row_and_close(row, selected);
        }
    }
}

fn open_edit_text(state: &mut ManagerState<'_>, target: GlobalMountTextTarget) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    let global = &mut settings.mounts;
    let Some(plan) = global_mount_selected_edit_text_plan(&global.pending, global.selected, target)
    else {
        return;
    };
    global.modal = Some(text_modal(plan.target, plan.label, &plan.initial));
}

fn open_settings_env_enter_modal(settings: &mut crate::tui::state::SettingsState<'_>) {
    let plan = settings_update::settings_env_selected_enter_plan(
        &settings.env.pending,
        &settings.env.expanded,
        settings.env.selected,
    );
    match plan {
        SettingsEnvEnterPlan::EditValue { scope, key } => {
            let plan = settings_env_value_edit_text_plan(&settings.env.pending, scope, key);
            let state = settings_env_text_input_state(&plan.target, plan.label, plan.current);
            settings.env.modal = Some(SettingsEnvModal::Text {
                target: plan.target,
                state: Box::new(state),
            });
        }
        SettingsEnvEnterPlan::OpenScopePicker => {
            settings.env.modal = Some(SettingsEnvModal::ScopePicker {
                state: settings_env_scope_picker_state(),
            });
        }
        SettingsEnvEnterPlan::ExpandRole(role) => {
            settings.env.expand_role(role);
        }
        SettingsEnvEnterPlan::AddRoleKey { scope } => {
            let plan = settings_env_new_key_text_plan(scope);
            let state =
                settings_env_key_input_state(&settings.env.pending, &plan.scope, plan.label, "");
            settings.env.modal = Some(SettingsEnvModal::Text {
                target: plan.target,
                state: Box::new(state),
            });
        }
        SettingsEnvEnterPlan::Noop => {}
    }
}

fn open_settings_env_add_modal(settings: &mut crate::tui::state::SettingsState<'_>) {
    let Some(scope) = settings_update::settings_env_selected_add_target(
        &settings.env.pending,
        &settings.env.expanded,
        settings.env.selected,
    ) else {
        return;
    };
    let plan = settings_env_new_key_text_plan(scope);
    let state = settings_env_key_input_state(&settings.env.pending, &plan.scope, plan.label, "");
    settings.env.modal = Some(SettingsEnvModal::Text {
        target: plan.target,
        state: Box::new(state),
    });
}

fn open_settings_env_delete_confirm(settings: &mut crate::tui::state::SettingsState<'_>) {
    let Some(key) = settings_update::settings_env_selected_delete_key(
        &settings.env.pending,
        &settings.env.expanded,
        settings.env.selected,
    ) else {
        return;
    };
    settings.env.modal = Some(SettingsEnvModal::Confirm {
        action: SettingsEnvConfirm::Delete,
        state: settings_env_delete_confirm_state(&key),
    });
}

fn toggle_settings_env_mask(settings: &mut crate::tui::state::SettingsState<'_>) {
    settings_update::toggle_selected_settings_env_maskable_value(
        &mut settings.env.unmasked_rows,
        &settings.env.pending,
        &settings.env.expanded,
        settings.env.selected,
    );
}

fn open_settings_env_picker_modal(
    settings: &mut crate::tui::state::SettingsState<'_>,
    op_cache: std::rc::Rc<std::cell::RefCell<jackin_env::OpCache>>,
) {
    let Some(target) = settings_update::settings_env_selected_picker_target(
        &settings.env.pending,
        &settings.env.expanded,
        settings.env.selected,
    ) else {
        return;
    };
    settings.env.set_pending_picker_target(target);
    settings.env.modal = Some(SettingsEnvModal::OpPicker {
        state: Box::new(crate::tui::op_picker::OpPickerState::new_with_cache(
            op_cache,
        )),
    });
}

fn delete_selected_settings_env(env: &mut crate::tui::state::SettingsEnvState<'_>) {
    env.remove_selected_row();
}

fn set_settings_env_value_typed(
    env: &mut crate::tui::state::SettingsEnvState<'_>,
    scope: &SettingsEnvScope,
    key: &str,
    value: jackin_core::EnvValue,
) {
    env.set_value(scope, key, value);
}

/// Promote any pending error from a settings sub-tab to `settings.error_popup`,
/// pop back to the workspace list when a handler set `exit_requested`.
pub fn after_settings_event(state: &mut ManagerState<'_>) {
    let outcome = {
        let ManagerStage::Settings(settings) = &mut state.stage else {
            return;
        };
        settings.take_after_event_outcome()
    };
    if let Some(msg) = outcome.error {
        dispatch_manager(
            state,
            ManagerMessage::OpenSettingsErrorPopup {
                title: settings_error_popup_title().into(),
                message: msg,
            },
        );
    }
    if outcome.exit_requested {
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
    settings: &mut crate::tui::state::SettingsState<'_>,
    choice: crate::tui::components::scope_picker::ScopeChoice,
) -> SettingsModalOutcome {
    match settings_update::global_mount_scope_picker_commit_plan(choice) {
        GlobalMountScopePickerCommitPlan::ApplyAllAgentsScope => {
            commit_text(&mut settings.mounts, &GlobalMountTextTarget::AddScope, "")
        }
        GlobalMountScopePickerCommitPlan::OpenRolePicker => {
            open_global_mount_role_picker(settings);
            SettingsModalOutcome::Continue
        }
    }
}

fn open_global_mount_role_picker(settings: &mut crate::tui::state::SettingsState<'_>) {
    match settings_update::global_mount_role_picker_open_plan(&settings.trust.pending) {
        RolePickerOpenPlan::NoRoles => {
            settings
                .mounts
                .set_error(settings_no_registered_roles_error_message());
        }
        RolePickerOpenPlan::Open(roles) => {
            settings
                .mounts
                .open_sub_modal(GlobalMountModal::RolePicker {
                    state: RolePickerState::new(roles),
                });
        }
    }
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
    label: impl Into<String>,
    initial: impl Into<String>,
) -> SettingsEnvModal<'static> {
    let state = settings_env_text_input_state(&target, label, initial);
    SettingsEnvModal::Text {
        target,
        state: Box::new(state),
    }
}

#[cfg(test)]
mod tests;
