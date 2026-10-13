// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Settings key entry points.

use super::{
    handle_auth_key, handle_env_key, handle_general_key, handle_global_mounts_key, handle_trust_key,
};
use crossterm::event::KeyEvent;

use crate::tui::keymap::{
    SETTINGS_CONTENT_SHELL_KEYMAP, SETTINGS_TAB_BAR_KEYMAP, SettingsContentShellAction,
    SettingsTabBarAction, bridged_keymap_action,
};

use crate::tui::screens::settings::update as settings_update;
use crate::tui::screens::settings::update::SettingsEnvHeaderKeyPlan;

use crate::tui::state::update::{ManagerMessage, update_manager};
use crate::tui::state::{ManagerStage, ManagerState, SettingsTab};

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

    let event = termrock::input::KeyEvent::from(key);
    let tab_bar_focused = settings.tab_bar_focused();
    let auth_kind_selected = settings.auth.has_selected_kind();
    let active_tab = settings.active_tab;

    // Shell: tab-bar navigation takes priority over per-tab dispatch.
    if tab_bar_focused {
        match bridged_keymap_action(&SETTINGS_TAB_BAR_KEYMAP, event) {
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
        match bridged_keymap_action(&SETTINGS_CONTENT_SHELL_KEYMAP, event) {
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

pub(crate) fn dispatch_manager(state: &mut ManagerState<'_>, message: ManagerMessage) {
    update_manager(state, message);
}
