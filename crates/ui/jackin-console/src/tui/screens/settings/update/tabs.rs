// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Settings tab navigation plans.

use super::super::model::SettingsTab;

#[must_use]
pub const fn previous_settings_tab(tab: SettingsTab) -> SettingsTab {
    match tab {
        SettingsTab::General => SettingsTab::Trust,
        SettingsTab::Mounts => SettingsTab::General,
        SettingsTab::Environments => SettingsTab::Mounts,
        SettingsTab::Auth => SettingsTab::Environments,
        SettingsTab::Trust => SettingsTab::Auth,
    }
}

#[must_use]
pub const fn next_settings_tab(tab: SettingsTab) -> SettingsTab {
    match tab {
        SettingsTab::General => SettingsTab::Mounts,
        SettingsTab::Mounts => SettingsTab::Environments,
        SettingsTab::Environments => SettingsTab::Auth,
        SettingsTab::Auth => SettingsTab::Trust,
        SettingsTab::Trust => SettingsTab::General,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsTabMovePlan {
    pub active_tab: SettingsTab,
    pub tab_bar_focused: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsShellKeyPlan {
    MoveTab { delta: isize, focus_tab_bar: bool },
    FocusContent,
    FocusTabBar { clear_auth_kind: bool },
    Continue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsTopLevelKeyPlan {
    MoveTab { delta: isize, focus_tab_bar: bool },
    FocusContent,
    FocusTabBar { clear_auth_kind: bool },
    SetEnvRoleExpanded { role: String, expanded: bool },
    Consume,
    Delegate(SettingsTab),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsGeneralKeyPlan {
    MoveSelection { delta: isize },
    ToggleSelected,
    ConfirmDiscard,
    ReturnToList,
    Save,
    Noop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsEnvKeyPlan {
    MoveSelection { delta: isize },
    ConfirmDiscard,
    ReturnToList,
    OpenAdd,
    Save,
    ConfirmDelete,
    ToggleMask,
    OpenPicker,
    OpenEnterModal,
    Noop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsEnvHeaderKeyPlan {
    SetExpanded { role: String, expanded: bool },
    Consume,
    Continue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTrustKeyPlan {
    MoveSelection { delta: isize },
    ScrollHorizontal { delta: i16 },
    ToggleSelected,
    ConfirmDiscard,
    ReturnToList,
    Save,
    Noop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsAuthKeyPlan {
    ClearKind,
    MoveSelection { delta: isize },
    EnterKind,
    ConfirmDiscard,
    ReturnToList,
    OpenForm,
    Save,
    Noop,
}

#[must_use]
pub const fn settings_tab_move_plan(
    active_tab: SettingsTab,
    delta: isize,
    focus_tab_bar: bool,
) -> SettingsTabMovePlan {
    SettingsTabMovePlan {
        active_tab: if delta.is_negative() {
            previous_settings_tab(active_tab)
        } else {
            next_settings_tab(active_tab)
        },
        tab_bar_focused: focus_tab_bar,
    }
}

#[must_use]
pub const fn settings_tab_select_plan(selected_tab: SettingsTab) -> SettingsTabMovePlan {
    SettingsTabMovePlan {
        active_tab: selected_tab,
        tab_bar_focused: true,
    }
}

#[must_use]
pub const fn settings_tab_bar_focus_plan(focused: bool) -> bool {
    focused
}
