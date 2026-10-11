// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Settings per-tab key plans.

use super::{
    SettingsAuthKeyPlan, SettingsEnvHeaderKeyPlan, SettingsEnvKeyPlan, SettingsFocusRegion,
    SettingsGeneralKeyPlan, SettingsShellKeyPlan, SettingsTopLevelKeyPlan, SettingsTrustKeyPlan,
    settings_env_flat_rows, settings_env_value, settings_focus_head, settings_focus_next,
    settings_focus_region, settings_focus_region_eq,
};
use std::collections::BTreeSet;

use super::super::model::{SettingsEnvConfig, SettingsEnvRow, SettingsTab};

use crossterm::event::KeyCode;
use jackin_core::EnvValue;

#[must_use]
pub const fn settings_shell_key_plan(
    key: KeyCode,
    tab_bar_focused: bool,
    auth_kind_selected: bool,
) -> SettingsShellKeyPlan {
    if tab_bar_focused {
        match key {
            KeyCode::Left | KeyCode::BackTab => {
                return SettingsShellKeyPlan::MoveTab {
                    delta: -1,
                    focus_tab_bar: true,
                };
            }
            KeyCode::Right => {
                return SettingsShellKeyPlan::MoveTab {
                    delta: 1,
                    focus_tab_bar: true,
                };
            }
            KeyCode::Tab | KeyCode::Down | KeyCode::Char('j' | 'J') => {
                // Tab from the tab bar walks the focus chain one region on.
                return match settings_focus_next(settings_focus_region(tab_bar_focused)) {
                    SettingsFocusRegion::Content => SettingsShellKeyPlan::FocusContent,
                    SettingsFocusRegion::TabBar => SettingsShellKeyPlan::Continue,
                };
            }
            _ => {}
        }
    }

    match key {
        KeyCode::Tab => SettingsShellKeyPlan::MoveTab {
            delta: 1,
            // Tab from the body wraps the chain back to the chain head.
            focus_tab_bar: settings_focus_region_eq(
                settings_focus_next(SettingsFocusRegion::Content),
                settings_focus_head(),
            ),
        },
        KeyCode::BackTab => SettingsShellKeyPlan::FocusTabBar {
            clear_auth_kind: false,
        },
        KeyCode::Esc if !tab_bar_focused => SettingsShellKeyPlan::FocusTabBar {
            clear_auth_kind: auth_kind_selected,
        },
        _ => SettingsShellKeyPlan::Continue,
    }
}

#[must_use]
pub const fn settings_general_key_plan(key: KeyCode, is_dirty: bool) -> SettingsGeneralKeyPlan {
    use crate::tui::screens::edit_save::{EditSaveDisposition, plan_leave_when_dirty};
    match key {
        KeyCode::Up | KeyCode::Char('k' | 'K') => {
            SettingsGeneralKeyPlan::MoveSelection { delta: -1 }
        }
        KeyCode::Down | KeyCode::Char('j' | 'J') => {
            SettingsGeneralKeyPlan::MoveSelection { delta: 1 }
        }
        KeyCode::Char(' ') => SettingsGeneralKeyPlan::ToggleSelected,
        KeyCode::Esc | KeyCode::Char('q' | 'Q') => match plan_leave_when_dirty(is_dirty) {
            EditSaveDisposition::ConfirmDiscard => SettingsGeneralKeyPlan::ConfirmDiscard,
            EditSaveDisposition::Noop | EditSaveDisposition::SaveNow => {
                SettingsGeneralKeyPlan::ReturnToList
            }
        },
        KeyCode::Char('s' | 'S') => SettingsGeneralKeyPlan::Save,
        _ => SettingsGeneralKeyPlan::Noop,
    }
}

#[expect(
    clippy::fn_params_excessive_bools,
    reason = "Four orthogonal env-key plan inputs (key, plain_modifier, ...) \
              — each is an independent input the env-key planner reads to route \
              the keypress to the right key-binding. Named-arg reads match the \
              per-input key-binding routing idiom."
)]
#[must_use]
pub const fn settings_env_key_plan(
    key: KeyCode,
    plain_modifier: bool,
    is_dirty: bool,
    op_available: bool,
    selected_is_op_ref: bool,
) -> SettingsEnvKeyPlan {
    use crate::tui::screens::edit_save::{EditSaveDisposition, plan_leave_when_dirty};
    match key {
        KeyCode::Up | KeyCode::Char('k' | 'K') => SettingsEnvKeyPlan::MoveSelection { delta: -1 },
        KeyCode::Down | KeyCode::Char('j' | 'J') => SettingsEnvKeyPlan::MoveSelection { delta: 1 },
        KeyCode::Esc | KeyCode::Char('q' | 'Q') => match plan_leave_when_dirty(is_dirty) {
            EditSaveDisposition::ConfirmDiscard => SettingsEnvKeyPlan::ConfirmDiscard,
            EditSaveDisposition::Noop | EditSaveDisposition::SaveNow => {
                SettingsEnvKeyPlan::ReturnToList
            }
        },
        KeyCode::Char('a' | 'A') => SettingsEnvKeyPlan::OpenAdd,
        KeyCode::Char('s' | 'S') => SettingsEnvKeyPlan::Save,
        KeyCode::Char('d' | 'D') if plain_modifier => SettingsEnvKeyPlan::ConfirmDelete,
        KeyCode::Char('m' | 'M') if plain_modifier => SettingsEnvKeyPlan::ToggleMask,
        KeyCode::Char('p' | 'P') if plain_modifier && op_available => {
            SettingsEnvKeyPlan::OpenPicker
        }
        KeyCode::Enter if selected_is_op_ref && op_available => SettingsEnvKeyPlan::OpenPicker,
        KeyCode::Enter => SettingsEnvKeyPlan::OpenEnterModal,
        _ => SettingsEnvKeyPlan::Noop,
    }
}

#[must_use]
pub const fn settings_auth_key_plan(
    key: KeyCode,
    is_dirty: bool,
    has_selected_kind: bool,
    selected_detail_row_is_focusable: bool,
) -> SettingsAuthKeyPlan {
    use crate::tui::screens::edit_save::{EditSaveDisposition, plan_leave_when_dirty};
    match key {
        KeyCode::Esc | KeyCode::Char('q' | 'Q') if has_selected_kind => {
            SettingsAuthKeyPlan::ClearKind
        }
        KeyCode::Up | KeyCode::Char('k' | 'K') => SettingsAuthKeyPlan::MoveSelection { delta: -1 },
        KeyCode::Down | KeyCode::Char('j' | 'J') => SettingsAuthKeyPlan::MoveSelection { delta: 1 },
        KeyCode::Enter if !has_selected_kind => SettingsAuthKeyPlan::EnterKind,
        KeyCode::Esc | KeyCode::Char('q' | 'Q') => match plan_leave_when_dirty(is_dirty) {
            EditSaveDisposition::ConfirmDiscard => SettingsAuthKeyPlan::ConfirmDiscard,
            EditSaveDisposition::Noop | EditSaveDisposition::SaveNow => {
                SettingsAuthKeyPlan::ReturnToList
            }
        },
        KeyCode::Enter if selected_detail_row_is_focusable => SettingsAuthKeyPlan::OpenForm,
        KeyCode::Char('s' | 'S') => SettingsAuthKeyPlan::Save,
        _ => SettingsAuthKeyPlan::Noop,
    }
}

#[must_use]
pub fn settings_env_header_key_plan(
    key: KeyCode,
    active_tab: SettingsTab,
    selected_row: Option<&SettingsEnvRow>,
) -> SettingsEnvHeaderKeyPlan {
    if active_tab != SettingsTab::Environments {
        return SettingsEnvHeaderKeyPlan::Continue;
    }

    match key {
        KeyCode::Right => match selected_row {
            Some(SettingsEnvRow::RoleHeader {
                role,
                expanded: false,
            }) => SettingsEnvHeaderKeyPlan::SetExpanded {
                role: role.clone(),
                expanded: true,
            },
            _ => SettingsEnvHeaderKeyPlan::Consume,
        },
        KeyCode::Left => match selected_row {
            Some(SettingsEnvRow::RoleHeader {
                role,
                expanded: true,
            }) => SettingsEnvHeaderKeyPlan::SetExpanded {
                role: role.clone(),
                expanded: false,
            },
            _ => SettingsEnvHeaderKeyPlan::Consume,
        },
        _ => SettingsEnvHeaderKeyPlan::Continue,
    }
}

#[must_use]
pub fn settings_env_selected_header_key_plan<V>(
    key: KeyCode,
    active_tab: SettingsTab,
    pending: &SettingsEnvConfig<V>,
    expanded_roles: &BTreeSet<String>,
    selected: usize,
) -> SettingsEnvHeaderKeyPlan {
    let rows = settings_env_flat_rows(pending, expanded_roles);
    settings_env_header_key_plan(key, active_tab, rows.get(selected))
}

#[must_use]
pub fn settings_top_level_key_plan<V>(
    key: KeyCode,
    active_tab: SettingsTab,
    tab_bar_focused: bool,
    auth_kind_selected: bool,
    env_pending: &SettingsEnvConfig<V>,
    env_expanded_roles: &BTreeSet<String>,
    env_selected: usize,
) -> SettingsTopLevelKeyPlan {
    match settings_shell_key_plan(key, tab_bar_focused, auth_kind_selected) {
        SettingsShellKeyPlan::MoveTab {
            delta,
            focus_tab_bar,
        } => {
            return SettingsTopLevelKeyPlan::MoveTab {
                delta,
                focus_tab_bar,
            };
        }
        SettingsShellKeyPlan::FocusContent => {
            return SettingsTopLevelKeyPlan::FocusContent;
        }
        SettingsShellKeyPlan::FocusTabBar { clear_auth_kind } => {
            return SettingsTopLevelKeyPlan::FocusTabBar { clear_auth_kind };
        }
        SettingsShellKeyPlan::Continue => {}
    }

    match settings_env_selected_header_key_plan(
        key,
        active_tab,
        env_pending,
        env_expanded_roles,
        env_selected,
    ) {
        SettingsEnvHeaderKeyPlan::SetExpanded { role, expanded } => {
            SettingsTopLevelKeyPlan::SetEnvRoleExpanded { role, expanded }
        }
        SettingsEnvHeaderKeyPlan::Consume => SettingsTopLevelKeyPlan::Consume,
        SettingsEnvHeaderKeyPlan::Continue => SettingsTopLevelKeyPlan::Delegate(active_tab),
    }
}

#[must_use]
pub fn settings_env_selected_key_matches<V>(
    config: &SettingsEnvConfig<V>,
    rows: &[SettingsEnvRow],
    selected: usize,
    predicate: impl FnOnce(&V) -> bool,
) -> bool {
    matches!(
        rows.get(selected),
        Some(SettingsEnvRow::Key { scope, key })
            if settings_env_value(config, scope, key).is_some_and(predicate)
    )
}

#[must_use]
pub fn settings_env_selected_key_is_op_ref(
    config: &SettingsEnvConfig<EnvValue>,
    rows: &[SettingsEnvRow],
    selected: usize,
) -> bool {
    settings_env_selected_key_matches(config, rows, selected, |value| {
        matches!(value, EnvValue::OpRef(_))
    })
}

#[must_use]
pub fn settings_env_selected_is_op_ref(
    config: &SettingsEnvConfig<EnvValue>,
    expanded_roles: &BTreeSet<String>,
    selected: usize,
) -> bool {
    let rows = settings_env_flat_rows(config, expanded_roles);
    settings_env_selected_key_is_op_ref(config, &rows, selected)
}

#[must_use]
pub fn settings_env_delete_key_for_row(row: Option<&SettingsEnvRow>) -> Option<&str> {
    match row {
        Some(SettingsEnvRow::Key { key, .. }) => Some(key.as_str()),
        _ => None,
    }
}

#[must_use]
pub fn settings_env_selected_delete_key<V>(
    pending: &SettingsEnvConfig<V>,
    expanded_roles: &BTreeSet<String>,
    selected: usize,
) -> Option<String> {
    let rows = settings_env_flat_rows(pending, expanded_roles);
    settings_env_delete_key_for_row(rows.get(selected)).map(str::to_owned)
}

#[must_use]
pub const fn settings_trust_key_plan(key: KeyCode, is_dirty: bool) -> SettingsTrustKeyPlan {
    use crate::tui::screens::edit_save::{EditSaveDisposition, plan_leave_when_dirty};
    match key {
        KeyCode::Up | KeyCode::Char('k' | 'K') => SettingsTrustKeyPlan::MoveSelection { delta: -1 },
        KeyCode::Down | KeyCode::Char('j' | 'J') => {
            SettingsTrustKeyPlan::MoveSelection { delta: 1 }
        }
        KeyCode::Char('h' | 'H') => SettingsTrustKeyPlan::ScrollHorizontal { delta: -8 },
        KeyCode::Char('l' | 'L') => SettingsTrustKeyPlan::ScrollHorizontal { delta: 8 },
        KeyCode::Char(' ') => SettingsTrustKeyPlan::ToggleSelected,
        KeyCode::Esc | KeyCode::Char('q' | 'Q') => match plan_leave_when_dirty(is_dirty) {
            EditSaveDisposition::ConfirmDiscard => SettingsTrustKeyPlan::ConfirmDiscard,
            EditSaveDisposition::Noop | EditSaveDisposition::SaveNow => {
                SettingsTrustKeyPlan::ReturnToList
            }
        },
        KeyCode::Char('s' | 'S') => SettingsTrustKeyPlan::Save,
        _ => SettingsTrustKeyPlan::Noop,
    }
}

#[must_use]
pub fn settings_tab_at_position(row: u16, col: u16) -> Option<SettingsTab> {
    let labels: Vec<&str> = SettingsTab::ALL.iter().map(|tab| tab.label()).collect();
    let idx = crate::tui::layout::tab_cell_at_position(row, col, &labels)?;
    SettingsTab::ALL.get(idx).copied()
}
