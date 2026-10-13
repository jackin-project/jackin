// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Settings general and trust key handling.

use super::{confirm_modal, dispatch_manager, open_settings_save_preview};
use crossterm::event::KeyEvent;

use crate::tui::keymap::{
    SETTINGS_GENERAL_TAB_KEYMAP, SETTINGS_TRUST_TAB_KEYMAP, SettingsGeneralTabAction,
    SettingsTrustTabAction, bridged_keymap_action,
};

use crate::tui::screens::settings::update as settings_update;

use crate::tui::state::update::ManagerMessage;
use crate::tui::state::{GlobalMountConfirm, ManagerStage, ManagerState};

pub(crate) fn handle_general_key(state: &mut ManagerState<'_>, key: KeyEvent) {
    let ManagerStage::Settings(settings) = &state.stage else {
        return;
    };
    let is_dirty = settings.is_dirty();
    let event = termrock::input::KeyEvent::from(key);
    match bridged_keymap_action(&SETTINGS_GENERAL_TAB_KEYMAP, event) {
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
                settings
                    .mounts
                    .modals
                    .open(confirm_modal(GlobalMountConfirm::Discard));
            } else {
                dispatch_manager(state, ManagerMessage::ReturnToList);
            }
        }
        None => {}
    }
}

pub(crate) fn handle_trust_key(state: &mut ManagerState<'_>, key: KeyEvent) {
    let term_size = state.cached_term_size;
    let term_width = term_size.width;
    let ManagerStage::Settings(settings) = &state.stage else {
        return;
    };
    let footer_h = settings.cached_footer_h;
    let is_dirty = settings.is_dirty();
    let content_width = settings_update::trust_content_width(&settings.trust);
    let event = termrock::input::KeyEvent::from(key);
    match bridged_keymap_action(&SETTINGS_TRUST_TAB_KEYMAP, event) {
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
                settings
                    .mounts
                    .modals
                    .open(confirm_modal(GlobalMountConfirm::Discard));
            } else {
                dispatch_manager(state, ManagerMessage::ReturnToList);
            }
        }
        None => {}
    }
}
