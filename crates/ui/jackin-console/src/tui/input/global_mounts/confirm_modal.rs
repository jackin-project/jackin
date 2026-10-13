// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Settings confirm modal handling.

use super::{
    SettingsModalOutcome, commit_add_scope_choice, commit_settings_confirm, commit_text,
    finalize_global_mount_add, request_settings_save, text_modal_for_target,
};
use crossterm::event::KeyEvent;

use crate::tui::components::file_browser::page_rows_for_modal;

use crate::tui::screens::settings::update as settings_update;
use crate::tui::screens::settings::update::GlobalMountRolePickerCommitPlan;
use crate::tui::screens::settings::view::{
    global_mount_add_draft_lost_message, settings_sensitive_paths_not_confirmed_message,
};

use crate::tui::state::{GlobalMountTextTarget, SettingsModal};
use crate::tui::update::{
    ConfirmSaveModalPlan, FileBrowserModalPlan, InlinePickerPlan, MountDstChoicePlan,
    ScopePickerPlan, confirm_save_modal_plan, file_browser_modal_plan, inline_picker_plan,
    mount_dst_choice_plan, scope_picker_plan,
};

pub fn handle_settings_confirm_modal(
    settings: &mut crate::tui::state::SettingsState<'_>,
    key: KeyEvent,
    term_size: ratatui::layout::Rect,
) -> SettingsModalOutcome {
    let Some(modal) = settings.mounts.modals.take_current() else {
        return SettingsModalOutcome::Continue;
    };
    let mut outcome = SettingsModalOutcome::Continue;
    match modal {
        SettingsModal::MountText { target, mut state } => {
            match inline_picker_plan(state.handle_key(key.into())) {
                InlinePickerPlan::Commit(value) => {
                    let committed_target = target.clone();
                    settings
                        .mounts
                        .modals
                        .open(SettingsModal::MountText { target, state });
                    outcome = commit_text(&mut settings.mounts, &committed_target, &value);
                }
                InlinePickerPlan::Dismiss => {
                    settings
                        .mounts
                        .pop_modal_chain_and_clear_add_draft_if_closed();
                }
                InlinePickerPlan::Continue => {
                    settings
                        .mounts
                        .modals
                        .open(SettingsModal::MountText { target, state });
                }
            }
        }
        SettingsModal::MountFileBrowser { mut state } => {
            let page_rows = page_rows_for_modal(term_size, &state);
            let browser_outcome = state.handle_key_with_page_rows(key, Some(page_rows));
            match file_browser_modal_plan(browser_outcome) {
                FileBrowserModalPlan::Dismiss => {
                    settings
                        .mounts
                        .pop_modal_chain_and_clear_add_draft_if_closed();
                }
                FileBrowserModalPlan::ResolveGitUrl(path) => {
                    settings
                        .mounts
                        .modals
                        .open(SettingsModal::MountFileBrowser { state });
                    outcome = SettingsModalOutcome::ResolveFileBrowserGitUrl(path);
                }
                FileBrowserModalPlan::OpenUrl(url) => {
                    settings
                        .mounts
                        .modals
                        .open(SettingsModal::MountFileBrowser { state });
                    outcome = SettingsModalOutcome::OpenUrl(url);
                }
                FileBrowserModalPlan::Continue => {
                    settings
                        .mounts
                        .modals
                        .open(SettingsModal::MountFileBrowser { state });
                }
                FileBrowserModalPlan::ApplyFileBrowserOutcome(browser_outcome) => {
                    settings
                        .mounts
                        .modals
                        .open(SettingsModal::MountFileBrowser { state });
                    outcome = SettingsModalOutcome::ApplyFileBrowserOutcome(browser_outcome);
                }
            }
        }
        SettingsModal::MountDstChoice { mut state } => {
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
                    settings
                        .mounts
                        .modals
                        .open(SettingsModal::MountDstChoice { state });
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
                    settings
                        .mounts
                        .modals
                        .open(SettingsModal::MountDstChoice { state });
                }
            }
        }
        SettingsModal::MountScopePicker { mut state } => {
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
                    settings
                        .mounts
                        .modals
                        .open(SettingsModal::MountScopePicker { state });
                }
            }
        }
        SettingsModal::MountRolePicker { state: mut picker } => {
            match inline_picker_plan(picker.handle_key(key)) {
                InlinePickerPlan::Commit(role) => {
                    match settings_update::global_mount_role_picker_commit_plan(
                        &mut settings.mounts.add_draft,
                        &role,
                    ) {
                        GlobalMountRolePickerCommitPlan::OpenFileBrowser => {
                            settings
                                .mounts
                                .modals
                                .open(SettingsModal::MountRolePicker { state: picker });
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
                    settings
                        .mounts
                        .modals
                        .open(SettingsModal::MountRolePicker { state: picker });
                }
            }
        }
        SettingsModal::MountConfirm { action, mut state } => {
            match settings_update::settings_confirm_plan(action, state.handle_key(key.into())) {
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
                    settings
                        .mounts
                        .modals
                        .open(SettingsModal::MountConfirm { action, state });
                }
            }
        }
        SettingsModal::MountPreviewSave { mut state } => {
            match confirm_save_modal_plan(state.handle_key(key)) {
                ConfirmSaveModalPlan::Commit => {
                    outcome = request_settings_save(settings);
                }
                ConfirmSaveModalPlan::Dismiss => settings.mounts.clear_modal_chain(),
                ConfirmSaveModalPlan::Continue => {
                    settings
                        .mounts
                        .modals
                        .open(SettingsModal::MountPreviewSave { state });
                }
            }
        }
        _ => unreachable!("mount input handler received a non-mount settings modal"),
    }
    outcome
}
