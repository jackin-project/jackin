// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Settings env tab key handling.

use super::{
    confirm_modal, dispatch_manager, open_settings_env_add_modal, open_settings_env_delete_confirm,
    open_settings_env_enter_modal, open_settings_env_picker_modal, open_settings_save_preview,
    toggle_settings_env_mask,
};
use crossterm::event::{KeyEvent, KeyModifiers};

use crate::tui::keymap::{SETTINGS_ENV_TAB_KEYMAP, SettingsEnvTabAction, bridged_keymap_action};

use crate::tui::screens::settings::update as settings_update;

use crate::tui::state::update::ManagerMessage;
use crate::tui::state::{GlobalMountConfirm, ManagerStage, ManagerState};

pub(crate) fn handle_env_key(state: &mut ManagerState<'_>, key: KeyEvent) {
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
    let event = termrock::input::KeyEvent::from(key);
    match bridged_keymap_action(&SETTINGS_ENV_TAB_KEYMAP, event) {
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
                settings
                    .mounts
                    .modals
                    .open(confirm_modal(GlobalMountConfirm::Discard));
            } else {
                dispatch_manager(state, ManagerMessage::ReturnToList);
            }
        }
        // Context check failed (Delete/ToggleMask/OpenPicker without plain_modifier) or no binding.
        Some(_) | None => {}
    }
}
