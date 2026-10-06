// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Global-mounts tab key handling.

use super::{
    confirm_modal, dispatch_manager, open_edit_text, open_global_mount_scope_picker,
    open_settings_save_preview,
};
use crossterm::event::KeyEvent;

use crate::tui::keymap::{
    SETTINGS_GLOBAL_MOUNTS_TAB_KEYMAP, SettingsGlobalMountsTabAction, bridged_keymap_action,
};
use crate::tui::mount_display::settings_global_config_mounts_content_width_with_cache;
use crate::tui::screens::settings::update as settings_update;
use crate::tui::screens::settings::update::GlobalMountGithubOpenPlan;
use crate::tui::screens::settings::view::global_mount_no_github_url_message;
use crate::tui::state::ManagerEffect;
use crate::tui::state::update::ManagerMessage;
use crate::tui::state::{GlobalMountConfirm, GlobalMountTextTarget, ManagerStage, ManagerState};

pub(crate) fn handle_global_mounts_key(state: &mut ManagerState<'_>, key: KeyEvent) {
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
    let event = termrock::input::KeyEvent::from(key);
    match bridged_keymap_action(&SETTINGS_GLOBAL_MOUNTS_TAB_KEYMAP, event) {
        Some(SettingsGlobalMountsTabAction::Save) => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                return;
            };
            if has_sensitive_mount {
                settings
                    .mounts
                    .modals
                    .open(confirm_modal(GlobalMountConfirm::Sensitive));
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
                settings
                    .mounts
                    .modals
                    .open(confirm_modal(GlobalMountConfirm::Discard));
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
            settings
                .mounts
                .modals
                .open(confirm_modal(GlobalMountConfirm::Remove));
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
