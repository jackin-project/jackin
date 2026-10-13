// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Keyboard-help overlay (`?` from any console stage) — builds the merged
//! [`HelpEntry`] list for the active route from the live console keymaps.
//!
//! TODO(keyboard-help-mouse): pointer input is not routed to the overlay yet —
//! see TODO.md "Follow-ups" → "keyboard-help-mouse".

use termrock::keymap::{KeyBinding, Keymap};
use termrock::style::DesignSystem;
use termrock::widgets::{HelpEntry, help_entries_from_keymap, merge_help_entries};

use crate::tui::keymap::{
    AUTH_MANAGE_KEYMAP, CONSOLE_GLOBAL_KEYMAP, EDITOR_CONTENT_KEYMAP, EDITOR_GLOBAL_KEYMAP,
    EDITOR_TAB_BAR_KEYMAP, PREVIEW_PANE_KEYMAP, SETTINGS_CONTENT_SHELL_KEYMAP,
    SETTINGS_ENV_TAB_KEYMAP, SETTINGS_GENERAL_TAB_KEYMAP, SETTINGS_GLOBAL_MOUNTS_TAB_KEYMAP,
    SETTINGS_TAB_BAR_KEYMAP, SETTINGS_TRUST_TAB_KEYMAP, WORKSPACE_LIST_KEYMAP,
};
use crate::tui::screens::settings::model::SettingsTab;
use crate::tui::state::{ManagerStage, ManagerState};

fn entries_from<A>(map: &'static Keymap<A>, system: &DesignSystem, category: &str) -> Vec<HelpEntry>
where
    A: Clone + Copy + PartialEq + 'static,
{
    help_entries_from_keymap(map, system, move |_action, binding: &KeyBinding<A>| {
        let label = binding.hint().unwrap_or("action").to_owned();
        (
            format!("{category}:{label}"),
            category.to_owned(),
            label,
            None,
            None,
            50,
        )
    })
}

/// Merged help entries for the route `state` currently shows. Every route
/// also contributes the console-global bindings (`? help`). Pure function —
/// the renderer and the dispatcher call it per event/frame.
#[must_use]
pub fn console_help_entries(state: &ManagerState<'_>, system: &DesignSystem) -> Vec<HelpEntry> {
    let mut parts: Vec<Vec<HelpEntry>> = Vec::new();
    match &state.stage {
        ManagerStage::List => {
            parts.push(entries_from(
                &WORKSPACE_LIST_KEYMAP,
                system,
                "Workspace list",
            ));
            parts.push(entries_from(&PREVIEW_PANE_KEYMAP, system, "Preview pane"));
        }
        ManagerStage::Editor(_) => {
            parts.push(entries_from(&EDITOR_GLOBAL_KEYMAP, system, "Editor"));
            parts.push(entries_from(&EDITOR_TAB_BAR_KEYMAP, system, "Editor"));
            parts.push(entries_from(&EDITOR_CONTENT_KEYMAP, system, "Editor"));
        }
        ManagerStage::Settings(settings) => {
            parts.push(entries_from(&SETTINGS_TAB_BAR_KEYMAP, system, "Settings"));
            parts.push(entries_from(
                &SETTINGS_CONTENT_SHELL_KEYMAP,
                system,
                "Settings",
            ));
            let tab = match settings.active_tab {
                SettingsTab::General => {
                    entries_from(&SETTINGS_GENERAL_TAB_KEYMAP, system, "Settings")
                }
                SettingsTab::Mounts => {
                    entries_from(&SETTINGS_GLOBAL_MOUNTS_TAB_KEYMAP, system, "Settings")
                }
                SettingsTab::Environments => {
                    entries_from(&SETTINGS_ENV_TAB_KEYMAP, system, "Settings")
                }
                SettingsTab::Trust => entries_from(&SETTINGS_TRUST_TAB_KEYMAP, system, "Settings"),
                SettingsTab::Auth => entries_from(&AUTH_MANAGE_KEYMAP, system, "Settings"),
            };
            parts.push(tab);
        }
        ManagerStage::CreatePrelude(_)
        | ManagerStage::ConfirmDelete { .. }
        | ManagerStage::ConfirmInstancePurge { .. } => {}
    }
    parts.push(entries_from(&CONSOLE_GLOBAL_KEYMAP, system, "Global"));
    merge_help_entries(parts)
}

#[cfg(test)]
mod tests;
