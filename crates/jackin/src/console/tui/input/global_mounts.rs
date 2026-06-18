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
    SettingsAuthModal, SettingsEnvConfirm, SettingsEnvEnterPlan, SettingsEnvModal,
    SettingsEnvScope, SettingsEnvTextTarget, SettingsTab,
};
use jackin_console::tui::auth_config::{
    apply_settings_auth_env_commit, clear_settings_auth_env_values,
};
use jackin_console::tui::components::auth_panel::{
    AuthFormKeyPlan, auth_credential_input_state, auth_form_key_plan_with_source_folder,
    auth_source_picker_state, generated_token_op_item_name, generated_token_source_picker_state,
};
use jackin_console::tui::components::file_browser::page_rows_for_modal;
use jackin_console::tui::mount_display::settings_global_config_mounts_content_width_with_cache;
use jackin_console::tui::screens::settings::update as settings_update;
use jackin_console::tui::screens::settings::update::{
    GlobalMountAddFinalizePlan, GlobalMountAddTextApplyPlan, GlobalMountScopePickerCommitPlan,
    GlobalMountTextCommitPlan, SettingsEnvHeaderKeyPlan, SettingsEnvKeyPlan,
    SettingsEnvOpPickerCommitPlan, SettingsEnvScopePickerCommitPlan,
    SettingsEnvScopePickerSelection, SettingsEnvSourcePickerCommitPlan,
    SettingsEnvSourcePickerSelection, SettingsEnvTextCommitPlan, SettingsGeneralKeyPlan,
    SettingsGlobalMountsKeyPlan, SettingsShellKeyPlan, SettingsTrustKeyPlan,
};
use jackin_console::tui::screens::settings::view::{
    global_mount_add_draft_lost_message, global_mount_confirm_state,
    global_mount_destination_empty_message, global_mount_gone_message,
    global_mount_name_empty_message, global_mount_no_github_url_message,
    global_mount_scope_picker_state, global_mount_selected_edit_text_plan,
    global_mount_text_input_state, global_mount_text_target_label,
    settings_auth_op_read_failed_message, settings_env_delete_confirm_state,
    settings_env_empty_key_error_message, settings_env_empty_key_text_plan,
    settings_env_key_input_state, settings_env_new_key_after_picker_text_plan,
    settings_env_new_key_text_plan, settings_env_plain_value_text_plan,
    settings_env_scope_picker_state, settings_env_source_picker_state,
    settings_env_text_input_state, settings_env_value_edit_text_plan, settings_error_popup_title,
    settings_no_registered_roles_error_message, settings_sensitive_paths_not_confirmed_message,
};
use jackin_console::tui::update::{
    BoolConfirmModalPlan, ConfirmSaveModalPlan, FileBrowserModalPlan, InlinePickerPlan,
    MountDstChoicePlan, ScopePickerPlan, SourcePickerPlan, bool_confirm_modal_plan,
    confirm_save_modal_plan, file_browser_modal_plan, inline_picker_plan, mount_dst_choice_plan,
    scope_picker_plan, source_picker_plan,
};

pub(super) type SettingsModalOutcome = jackin_console::tui::message::ConsoleSettingsModalOutcome;

pub(super) type SettingsAuthOutcome =
    jackin_console::tui::message::ConsoleSettingsAuthOutcome<jackin_core::OpRef>;

#[cfg(test)]
pub(super) fn handle_settings_key(state: &mut ManagerState<'_>, key: KeyEvent) {
    handle_settings_key_with_effects(state, key);
}

pub(super) fn handle_settings_key_with_effects(state: &mut ManagerState<'_>, key: KeyEvent) {
    let ManagerStage::Settings(settings) = &state.stage else {
        return;
    };

    match settings_update::settings_shell_key_plan(
        key.code,
        settings.tab_bar_focused(),
        settings.auth.selected_kind.is_some(),
    ) {
        SettingsShellKeyPlan::MoveTab {
            delta,
            focus_tab_bar,
        } => {
            dispatch_manager(
                state,
                ManagerMessage::MoveSettingsTab {
                    delta,
                    focus_tab_bar,
                },
            );
            return;
        }
        SettingsShellKeyPlan::FocusContent => {
            dispatch_manager(state, ManagerMessage::FocusSettingsContent);
            return;
        }
        SettingsShellKeyPlan::FocusTabBar { clear_auth_kind } => {
            if clear_auth_kind {
                dispatch_manager(state, ManagerMessage::ClearSettingsAuthKind);
            }
            dispatch_manager(state, ManagerMessage::FocusSettingsTabBar);
            return;
        }
        SettingsShellKeyPlan::Continue => {}
    }

    let ManagerStage::Settings(settings) = &state.stage else {
        return;
    };
    match settings_update::settings_env_selected_header_key_plan(
        key.code,
        settings.active_tab,
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
        SettingsEnvHeaderKeyPlan::Consume => {
            return;
        }
        SettingsEnvHeaderKeyPlan::Continue => {}
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

fn handle_global_mounts_key(state: &mut ManagerState<'_>, key: KeyEvent) {
    let ManagerStage::Settings(settings) = &state.stage else {
        return;
    };
    let plan = settings_update::settings_global_mounts_key_plan(
        key.code,
        settings.is_dirty(),
        jackin_console::services::workspace::global_rows_have_sensitive_mount(
            &settings.mounts.pending,
        ),
        settings.mounts.selected,
        settings.mounts.pending.len(),
    );
    let term_width = state.cached_term_size.width;
    let content_width = settings_global_config_mounts_content_width_with_cache(
        &settings.mounts.pending,
        &settings.mounts.mount_info_cache,
    );
    let footer_h = settings.cached_footer_h;
    match plan {
        SettingsGlobalMountsKeyPlan::ConfirmSensitiveSave => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                return;
            };
            settings.mounts.modal = Some(confirm_modal(GlobalMountConfirm::Sensitive));
        }
        SettingsGlobalMountsKeyPlan::OpenSavePreview => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                return;
            };
            open_settings_save_preview(settings);
        }
        SettingsGlobalMountsKeyPlan::ScrollHorizontal { delta } => {
            dispatch_manager(
                state,
                ManagerMessage::ScrollSettingsGlobalMountsHorizontal {
                    delta,
                    term_width,
                    content_width,
                },
            );
        }
        SettingsGlobalMountsKeyPlan::MoveSelection { delta } => {
            dispatch_manager(
                state,
                ManagerMessage::MoveSettingsGlobalMountsSelection {
                    delta,
                    term: state.cached_term_size,
                    footer_h,
                },
            );
        }
        SettingsGlobalMountsKeyPlan::ToggleReadonly => {
            dispatch_manager(state, ManagerMessage::ToggleSettingsGlobalMountReadonly);
        }
        SettingsGlobalMountsKeyPlan::ConfirmDiscard => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                return;
            };
            settings.mounts.modal = Some(confirm_modal(GlobalMountConfirm::Discard));
        }
        SettingsGlobalMountsKeyPlan::ReturnToList => {
            dispatch_manager(state, ManagerMessage::ReturnToList);
        }
        SettingsGlobalMountsKeyPlan::OpenAdd => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                return;
            };
            open_global_mount_scope_picker(&mut settings.mounts);
        }
        SettingsGlobalMountsKeyPlan::ConfirmRemove => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                return;
            };
            settings.mounts.modal = Some(confirm_modal(GlobalMountConfirm::Remove));
        }
        SettingsGlobalMountsKeyPlan::OpenGithub => {
            let open_url = {
                let ManagerStage::Settings(settings) = &mut state.stage else {
                    return;
                };
                let global = &mut settings.mounts;
                if let Some(row) = global.pending.get(global.selected) {
                    if let Some(web_url) = global.mount_info_cache.github_web_url(&row.mount.src) {
                        Some(web_url)
                    } else {
                        global.error = Some(global_mount_no_github_url_message().into());
                        None
                    }
                } else {
                    None
                }
            };
            if let Some(web_url) = open_url {
                state.request_effect(ManagerEffect::OpenUrl(web_url));
            }
        }
        SettingsGlobalMountsKeyPlan::OpenEdit(target) => open_edit_text(state, target),
        SettingsGlobalMountsKeyPlan::Noop => {}
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
    let selected_is_op_ref = settings_update::settings_env_selected_is_op_ref(
        &settings.env.pending,
        &settings.env.expanded,
        settings.env.selected,
    );
    let plan = settings_update::settings_env_key_plan(
        key.code,
        (key.modifiers - KeyModifiers::SHIFT).is_empty(),
        settings.is_dirty(),
        op_available,
        selected_is_op_ref,
    );
    match plan {
        SettingsEnvKeyPlan::MoveSelection { delta } => {
            dispatch_manager(
                state,
                ManagerMessage::MoveSettingsEnvSelection {
                    delta,
                    term: term_size,
                    footer_h,
                },
            );
        }
        SettingsEnvKeyPlan::ConfirmDiscard => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                return;
            };
            settings.mounts.modal = Some(confirm_modal(GlobalMountConfirm::Discard));
        }
        SettingsEnvKeyPlan::ReturnToList => {
            dispatch_manager(state, ManagerMessage::ReturnToList);
        }
        SettingsEnvKeyPlan::OpenAdd => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                return;
            };
            open_settings_env_add_modal(settings);
        }
        SettingsEnvKeyPlan::Save => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                return;
            };
            open_settings_save_preview(settings);
        }
        SettingsEnvKeyPlan::ConfirmDelete => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                return;
            };
            open_settings_env_delete_confirm(settings);
        }
        SettingsEnvKeyPlan::ToggleMask => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                return;
            };
            toggle_settings_env_mask(settings);
        }
        SettingsEnvKeyPlan::OpenPicker => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                return;
            };
            open_settings_env_picker_modal(settings, op_cache);
        }
        SettingsEnvKeyPlan::OpenEnterModal => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                return;
            };
            open_settings_env_enter_modal(settings);
        }
        SettingsEnvKeyPlan::Noop => {}
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
    let ManagerStage::Settings(settings) = &state.stage else {
        return;
    };
    match settings_update::settings_general_key_plan(key.code, settings.is_dirty()) {
        SettingsGeneralKeyPlan::MoveSelection { delta } => {
            dispatch_manager(
                state,
                ManagerMessage::MoveSettingsGeneralSelection { delta },
            );
        }
        SettingsGeneralKeyPlan::ToggleSelected => {
            dispatch_manager(state, ManagerMessage::ToggleSettingsGeneralSelected);
        }
        SettingsGeneralKeyPlan::ConfirmDiscard => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                return;
            };
            settings.mounts.modal = Some(confirm_modal(GlobalMountConfirm::Discard));
        }
        SettingsGeneralKeyPlan::ReturnToList => {
            dispatch_manager(state, ManagerMessage::ReturnToList);
        }
        SettingsGeneralKeyPlan::Save => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                return;
            };
            open_settings_save_preview(settings);
        }
        SettingsGeneralKeyPlan::Noop => {}
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
    match settings_update::settings_trust_key_plan(key.code, settings.is_dirty()) {
        SettingsTrustKeyPlan::MoveSelection { delta } => {
            dispatch_manager(
                state,
                ManagerMessage::MoveSettingsTrustSelection {
                    delta,
                    term: term_size,
                    footer_h,
                },
            );
        }
        SettingsTrustKeyPlan::ScrollHorizontal { delta } => {
            dispatch_manager(
                state,
                ManagerMessage::ScrollSettingsTrustHorizontal {
                    delta,
                    term_width,
                    content_width,
                },
            );
        }
        SettingsTrustKeyPlan::ToggleSelected => {
            dispatch_manager(state, ManagerMessage::ToggleSettingsTrustSelected);
        }
        SettingsTrustKeyPlan::ConfirmDiscard => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                return;
            };
            settings.mounts.modal = Some(confirm_modal(GlobalMountConfirm::Discard));
        }
        SettingsTrustKeyPlan::ReturnToList => {
            dispatch_manager(state, ManagerMessage::ReturnToList);
        }
        SettingsTrustKeyPlan::Save => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                return;
            };
            open_settings_save_preview(settings);
        }
        SettingsTrustKeyPlan::Noop => {}
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
        GlobalMountModal::Text { target, mut state } => {
            match inline_picker_plan(state.handle_key(key)) {
                InlinePickerPlan::Commit(value) => {
                    let committed_target = target.clone();
                    settings.mounts.modal = Some(GlobalMountModal::Text { target, state });
                    outcome = commit_text(&mut settings.mounts, &committed_target, &value);
                }
                InlinePickerPlan::Dismiss => {
                    settings.mounts.pop_modal_chain();
                    if settings.mounts.modal.is_none() {
                        settings.mounts.add_draft = None;
                    }
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
                    settings.mounts.pop_modal_chain();
                    if settings.mounts.modal.is_none() {
                        settings.mounts.add_draft = None;
                    }
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
                    if let Some(draft) = settings.mounts.add_draft.as_mut() {
                        draft.dst = src;
                    }
                    finalize_global_mount_add(&mut settings.mounts);
                }
                MountDstChoicePlan::OpenEditInput => {
                    if let Some(draft) = settings.mounts.add_draft.as_mut() {
                        draft.dst.clone_from(&src);
                    }
                    settings.mounts.modal = Some(GlobalMountModal::MountDstChoice { state });
                    settings.mounts.open_sub_modal(text_modal_for_target(
                        GlobalMountTextTarget::AddDestination,
                        &src,
                    ));
                }
                MountDstChoicePlan::Dismiss => {
                    settings.mounts.pop_modal_chain();
                    if settings.mounts.modal.is_none() {
                        settings.mounts.add_draft = None;
                    }
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
                    settings.mounts.pop_modal_chain();
                    if settings.mounts.modal.is_none() {
                        settings.mounts.add_draft = None;
                    }
                }
                ScopePickerPlan::Continue => {
                    settings.mounts.modal = Some(GlobalMountModal::ScopePicker { state });
                }
            }
        }
        GlobalMountModal::RolePicker { state: mut picker } => {
            match inline_picker_plan(picker.handle_key(key)) {
                InlinePickerPlan::Commit(role) => {
                    if let Some(draft) = settings.mounts.add_draft.as_mut() {
                        draft.scope = Some(role.key());
                        settings.mounts.modal =
                            Some(GlobalMountModal::RolePicker { state: picker });
                        outcome = SettingsModalOutcome::OpenGlobalMountFileBrowser;
                    } else {
                        settings.mounts.error = Some(global_mount_add_draft_lost_message().into());
                    }
                }
                InlinePickerPlan::Dismiss => {
                    settings.mounts.pop_modal_chain();
                    if settings.mounts.modal.is_none() {
                        settings.mounts.add_draft = None;
                    }
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
        SettingsEnvModal::Text { target, mut state } => {
            match inline_picker_plan(state.handle_key(key)) {
                InlinePickerPlan::Commit(value) => {
                    let committed_target = target.clone();
                    env.modal = Some(SettingsEnvModal::Text { target, state });
                    commit_env_text(env, &committed_target, &value);
                }
                InlinePickerPlan::Dismiss => {
                    env.pop_modal_chain();
                    if env.modal.is_none() {
                        env.pending_env_key = None;
                        env.pending_picker_value = None;
                    }
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
                    env.pop_modal_chain();
                    env.pending_env_key = None;
                    env.pending_picker_value = None;
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
                    crate::console::tui::op_picker::OpPickerSelection::NewItem { .. }
                    | crate::console::tui::op_picker::OpPickerSelection::EditItemField { .. },
                ) => unreachable!("settings-env OpPicker runs in Browse mode"),
                InlinePickerPlan::Commit(
                    crate::console::tui::op_picker::OpPickerSelection::Existing(op_ref),
                ) => {
                    let plan = settings_update::settings_env_op_picker_commit_plan(
                        env.pending_picker_target.as_ref(),
                    );
                    env.pending_picker_target.take();
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
                            env.pending_picker_value = Some(jackin_core::EnvValue::OpRef(op_ref));
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
                    env.pop_modal_chain();
                    env.pending_picker_target = None;
                    env.pending_picker_value = None;
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
    settings: &mut crate::console::tui::state::SettingsState<'_>,
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
            settings.mounts.pending.remove(remove_index);
            settings.mounts.selected = selected;
            SettingsModalOutcome::Continue
        }
        settings_update::SettingsConfirmCommitPlan::Save => request_settings_save(settings),
        settings_update::SettingsConfirmCommitPlan::OpenSavePreview => {
            open_settings_save_preview(settings);
            SettingsModalOutcome::Continue
        }
        settings_update::SettingsConfirmCommitPlan::DiscardAll => {
            settings.discard_all();
            settings.mounts.exit_requested = true;
            SettingsModalOutcome::Continue
        }
        settings_update::SettingsConfirmCommitPlan::Noop => SettingsModalOutcome::Continue,
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
    match settings_update::global_mount_text_commit_plan(target, value) {
        plan @ (GlobalMountTextCommitPlan::AddScope(_)
        | GlobalMountTextCommitPlan::AddName(_)
        | GlobalMountTextCommitPlan::AddSource(_)
        | GlobalMountTextCommitPlan::AddDestination(_)) => {
            return apply_global_mount_add_text(global, plan);
        }
        GlobalMountTextCommitPlan::SetSource(value) => {
            let Some(row) = global.pending.get_mut(global.selected) else {
                global.error = Some(global_mount_gone_message().into());
                return SettingsModalOutcome::Continue;
            };
            row.mount.src = value;
            global.clear_modal_chain();
        }
        GlobalMountTextCommitPlan::SetDestination(value) => {
            let Some(row) = global.pending.get_mut(global.selected) else {
                global.error = Some(global_mount_gone_message().into());
                return SettingsModalOutcome::Continue;
            };
            row.mount.dst = value;
            global.clear_modal_chain();
        }
        GlobalMountTextCommitPlan::SetScope(scope) => {
            let Some(row) = global.pending.get_mut(global.selected) else {
                global.error = Some(global_mount_gone_message().into());
                return SettingsModalOutcome::Continue;
            };
            row.scope = scope;
            global.clear_modal_chain();
        }
        GlobalMountTextCommitPlan::Rename(value) => {
            let Some(row) = global.pending.get_mut(global.selected) else {
                global.error = Some(global_mount_gone_message().into());
                return SettingsModalOutcome::Continue;
            };
            row.name = value;
            global.clear_modal_chain();
        }
        GlobalMountTextCommitPlan::EmptyName => {
            global.error = Some(global_mount_name_empty_message().into());
            return SettingsModalOutcome::Continue;
        }
    }
    SettingsModalOutcome::Continue
}

fn commit_env_text(
    env: &mut crate::console::tui::state::SettingsEnvState<'_>,
    target: &SettingsEnvTextTarget,
    value: &str,
) {
    match settings_update::settings_env_text_commit_plan(
        target,
        value,
        env.pending_picker_value.is_some(),
    ) {
        SettingsEnvTextCommitPlan::EmptyKey { scope } => {
            env.error = Some(settings_env_empty_key_error_message().into());
            let plan = settings_env_empty_key_text_plan(scope);
            let state = settings_env_key_input_state(&env.pending, &plan.scope, plan.label, "");
            env.modal = Some(SettingsEnvModal::Text {
                target: plan.target,
                state: Box::new(state),
            });
        }
        SettingsEnvTextCommitPlan::SetPendingPickerValue { scope, key } => {
            if let Some(stashed) = env.pending_picker_value.take() {
                set_settings_env_value_typed(env, &scope, &key, stashed);
                env.pending_env_key = None;
                env.clear_modal_chain();
            }
        }
        SettingsEnvTextCommitPlan::OpenSourcePicker { scope, key } => {
            env.pending_env_key = Some((scope, key.clone()));
            env.open_sub_modal(SettingsEnvModal::SourcePicker {
                state: settings_env_source_picker_state(key),
            });
        }
        SettingsEnvTextCommitPlan::SetPlainValue { scope, key, value } => {
            set_settings_env_value_typed(env, &scope, &key, jackin_core::EnvValue::Plain(value));
            env.pending_env_key = None;
            env.clear_modal_chain();
        }
    }
}

fn commit_settings_env_source_picker(
    env: &mut crate::console::tui::state::SettingsEnvState<'_>,
    selection: SettingsEnvSourcePickerSelection,
    source: jackin_console::tui::components::source_picker::SourcePickerState,
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
            env.pending_picker_target = Some((scope, Some(key)));
            env.pending_env_key = None;
            env.modal = Some(SettingsEnvModal::SourcePicker { state: source });
            env.open_sub_modal(SettingsEnvModal::OpPicker {
                state: Box::new(
                    crate::console::tui::op_picker::OpPickerState::new_with_cache(op_cache),
                ),
            });
        }
    }
}

fn commit_settings_env_scope_picker(
    env: &mut crate::console::tui::state::SettingsEnvState<'_>,
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

fn open_settings_env_role_picker(env: &mut crate::console::tui::state::SettingsEnvState<'_>) {
    use crate::console::tui::state::RolePickerState;

    let roles = settings_update::settings_env_role_picker_roles(&env.pending);
    if roles.is_empty() {
        env.error = Some(settings_no_registered_roles_error_message().into());
        return;
    }
    env.open_sub_modal(SettingsEnvModal::RolePicker {
        state: RolePickerState::new(roles),
    });
}

fn apply_global_mount_add_text(
    global: &mut crate::console::tui::state::GlobalMountsState<'_>,
    plan: GlobalMountTextCommitPlan,
) -> SettingsModalOutcome {
    match settings_update::global_mount_add_text_apply_plan(&mut global.add_draft, plan) {
        GlobalMountAddTextApplyPlan::MissingDraft => {
            global.error = Some(global_mount_add_draft_lost_message().into());
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

fn open_global_mount_scope_picker(global: &mut crate::console::tui::state::GlobalMountsState<'_>) {
    global.add_draft = Some(GlobalMountDraft::default());
    global.modal_parents.clear();
    global.modal = Some(scope_picker_modal());
}

fn finalize_global_mount_add(global: &mut crate::console::tui::state::GlobalMountsState<'_>) {
    let Some(draft) = global.add_draft.take() else {
        global.error = Some(global_mount_add_draft_lost_message().into());
        return;
    };
    match settings_update::global_mount_add_finalize_plan(&global.pending, draft) {
        GlobalMountAddFinalizePlan::EmptyDestination(draft) => {
            global.error = Some(global_mount_destination_empty_message().into());
            global.add_draft = Some(draft);
        }
        GlobalMountAddFinalizePlan::Add { row, selected } => {
            global.pending.push(row);
            global.selected = selected;
            global.clear_modal_chain();
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

fn open_settings_env_enter_modal(settings: &mut crate::console::tui::state::SettingsState<'_>) {
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
            settings.env.expanded.insert(role);
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

fn open_settings_env_add_modal(settings: &mut crate::console::tui::state::SettingsState<'_>) {
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

fn open_settings_env_delete_confirm(settings: &mut crate::console::tui::state::SettingsState<'_>) {
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

fn toggle_settings_env_mask(settings: &mut crate::console::tui::state::SettingsState<'_>) {
    settings_update::toggle_selected_settings_env_maskable_value(
        &mut settings.env.unmasked_rows,
        &settings.env.pending,
        &settings.env.expanded,
        settings.env.selected,
    );
}

fn open_settings_env_picker_modal(
    settings: &mut crate::console::tui::state::SettingsState<'_>,
    op_cache: std::rc::Rc<std::cell::RefCell<jackin_env::OpCache>>,
) {
    let Some(target) = settings_update::settings_env_selected_picker_target(
        &settings.env.pending,
        &settings.env.expanded,
        settings.env.selected,
    ) else {
        return;
    };
    settings.env.pending_picker_target = Some(target);
    settings.env.modal = Some(SettingsEnvModal::OpPicker {
        state: Box::new(crate::console::tui::op_picker::OpPickerState::new_with_cache(op_cache)),
    });
}

fn delete_selected_settings_env(env: &mut crate::console::tui::state::SettingsEnvState<'_>) {
    settings_update::remove_selected_settings_env_row(
        &mut env.pending,
        &env.expanded,
        &mut env.selected,
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

fn open_global_mount_role_picker(settings: &mut crate::console::tui::state::SettingsState<'_>) {
    let roles = settings_update::global_mount_role_picker_roles(&settings.trust.pending);
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
