// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn settings_tab_move_plan_cycles_and_sets_focus() {
    assert_eq!(
        settings_tab_move_plan(SettingsTab::Trust, 1, true),
        SettingsTabMovePlan {
            active_tab: SettingsTab::General,
            tab_bar_focused: true,
        }
    );
    assert_eq!(
        settings_tab_move_plan(SettingsTab::General, -1, false),
        SettingsTabMovePlan {
            active_tab: SettingsTab::Trust,
            tab_bar_focused: false,
        }
    );
}

#[test]
fn settings_tab_select_plan_focuses_selected_tab() {
    assert_eq!(
        settings_tab_select_plan(SettingsTab::Trust),
        SettingsTabMovePlan {
            active_tab: SettingsTab::Trust,
            tab_bar_focused: true,
        }
    );
}

#[test]
fn settings_tab_bar_focus_plan_returns_requested_focus() {
    assert!(settings_tab_bar_focus_plan(true));
    assert!(!settings_tab_bar_focus_plan(false));
}

#[test]
fn settings_focus_chain_walks_tab_bar_then_content_and_wraps() {
    assert_eq!(
        settings_focus_order(),
        [SettingsFocusRegion::TabBar, SettingsFocusRegion::Content]
    );
    assert_eq!(settings_focus_head(), SettingsFocusRegion::TabBar);
    assert_eq!(
        settings_focus_next(SettingsFocusRegion::TabBar),
        SettingsFocusRegion::Content
    );
    assert_eq!(
        settings_focus_next(SettingsFocusRegion::Content),
        SettingsFocusRegion::TabBar
    );
    assert_eq!(settings_focus_region(true), SettingsFocusRegion::TabBar);
    assert_eq!(settings_focus_region(false), SettingsFocusRegion::Content);
}

#[test]
fn settings_shell_key_plan_routes_tab_shell_keys_from_facts() {
    assert_eq!(
        settings_shell_key_plan(KeyCode::Left, true, false),
        SettingsShellKeyPlan::MoveTab {
            delta: -1,
            focus_tab_bar: true,
        }
    );
    assert_eq!(
        settings_shell_key_plan(KeyCode::Right, true, false),
        SettingsShellKeyPlan::MoveTab {
            delta: 1,
            focus_tab_bar: true,
        }
    );
    assert_eq!(
        settings_shell_key_plan(KeyCode::Down, true, false),
        SettingsShellKeyPlan::FocusContent
    );
    assert_eq!(
        settings_shell_key_plan(KeyCode::Char('J'), true, false),
        SettingsShellKeyPlan::FocusContent
    );
    assert_eq!(
        settings_shell_key_plan(KeyCode::Tab, false, false),
        SettingsShellKeyPlan::MoveTab {
            delta: 1,
            focus_tab_bar: true,
        }
    );
    assert_eq!(
        settings_shell_key_plan(KeyCode::BackTab, false, false),
        SettingsShellKeyPlan::FocusTabBar {
            clear_auth_kind: false,
        }
    );
    assert_eq!(
        settings_shell_key_plan(KeyCode::Esc, false, true),
        SettingsShellKeyPlan::FocusTabBar {
            clear_auth_kind: true,
        }
    );
    assert_eq!(
        settings_shell_key_plan(KeyCode::Char('s'), true, false),
        SettingsShellKeyPlan::Continue
    );
    assert_eq!(
        settings_shell_key_plan(KeyCode::Esc, true, true),
        SettingsShellKeyPlan::Continue
    );
}

#[test]
fn settings_general_key_plan_routes_keys_from_facts() {
    assert_eq!(
        settings_general_key_plan(KeyCode::Up, false),
        SettingsGeneralKeyPlan::MoveSelection { delta: -1 }
    );
    assert_eq!(
        settings_general_key_plan(KeyCode::Char('J'), false),
        SettingsGeneralKeyPlan::MoveSelection { delta: 1 }
    );
    assert_eq!(
        settings_general_key_plan(KeyCode::Char(' '), false),
        SettingsGeneralKeyPlan::ToggleSelected
    );
    assert_eq!(
        settings_general_key_plan(KeyCode::Esc, true),
        SettingsGeneralKeyPlan::ConfirmDiscard
    );
    assert_eq!(
        settings_general_key_plan(KeyCode::Esc, false),
        SettingsGeneralKeyPlan::ReturnToList
    );
    assert_eq!(
        settings_general_key_plan(KeyCode::Char('q'), true),
        SettingsGeneralKeyPlan::ConfirmDiscard
    );
    assert_eq!(
        settings_general_key_plan(KeyCode::Char('S'), false),
        SettingsGeneralKeyPlan::Save
    );
    assert_eq!(
        settings_general_key_plan(KeyCode::Char('x'), false),
        SettingsGeneralKeyPlan::Noop
    );
}

#[test]
fn settings_env_key_plan_routes_keys_from_facts() {
    assert_eq!(
        settings_env_key_plan(KeyCode::Up, true, false, false, false),
        SettingsEnvKeyPlan::MoveSelection { delta: -1 }
    );
    assert_eq!(
        settings_env_key_plan(KeyCode::Char('J'), true, false, false, false),
        SettingsEnvKeyPlan::MoveSelection { delta: 1 }
    );
    assert_eq!(
        settings_env_key_plan(KeyCode::Esc, true, true, false, false),
        SettingsEnvKeyPlan::ConfirmDiscard
    );
    assert_eq!(
        settings_env_key_plan(KeyCode::Esc, true, false, false, false),
        SettingsEnvKeyPlan::ReturnToList
    );
    assert_eq!(
        settings_env_key_plan(KeyCode::Char('a'), true, false, false, false),
        SettingsEnvKeyPlan::OpenAdd
    );
    assert_eq!(
        settings_env_key_plan(KeyCode::Char('S'), true, false, false, false),
        SettingsEnvKeyPlan::Save
    );
    assert_eq!(
        settings_env_key_plan(KeyCode::Char('d'), true, false, false, false),
        SettingsEnvKeyPlan::ConfirmDelete
    );
    assert_eq!(
        settings_env_key_plan(KeyCode::Char('d'), false, false, false, false),
        SettingsEnvKeyPlan::Noop
    );
    assert_eq!(
        settings_env_key_plan(KeyCode::Char('m'), true, false, false, false),
        SettingsEnvKeyPlan::ToggleMask
    );
    assert_eq!(
        settings_env_key_plan(KeyCode::Char('p'), true, false, true, false),
        SettingsEnvKeyPlan::OpenPicker
    );
    assert_eq!(
        settings_env_key_plan(KeyCode::Char('p'), true, false, false, false),
        SettingsEnvKeyPlan::Noop
    );
    assert_eq!(
        settings_env_key_plan(KeyCode::Enter, true, false, true, true),
        SettingsEnvKeyPlan::OpenPicker
    );
    assert_eq!(
        settings_env_key_plan(KeyCode::Enter, true, false, false, true),
        SettingsEnvKeyPlan::OpenEnterModal
    );
    assert_eq!(
        settings_env_key_plan(KeyCode::Char('x'), true, false, true, true),
        SettingsEnvKeyPlan::Noop
    );
}

#[test]
fn settings_auth_key_plan_routes_keys_from_facts() {
    assert_eq!(
        settings_auth_key_plan(KeyCode::Esc, true, true, true),
        SettingsAuthKeyPlan::ClearKind
    );
    assert_eq!(
        settings_auth_key_plan(KeyCode::Char('Q'), true, true, false),
        SettingsAuthKeyPlan::ClearKind
    );
    assert_eq!(
        settings_auth_key_plan(KeyCode::Up, false, false, false),
        SettingsAuthKeyPlan::MoveSelection { delta: -1 }
    );
    assert_eq!(
        settings_auth_key_plan(KeyCode::Char('J'), false, true, false),
        SettingsAuthKeyPlan::MoveSelection { delta: 1 }
    );
    assert_eq!(
        settings_auth_key_plan(KeyCode::Enter, false, false, false),
        SettingsAuthKeyPlan::EnterKind
    );
    assert_eq!(
        settings_auth_key_plan(KeyCode::Esc, true, false, false),
        SettingsAuthKeyPlan::ConfirmDiscard
    );
    assert_eq!(
        settings_auth_key_plan(KeyCode::Esc, false, false, false),
        SettingsAuthKeyPlan::ReturnToList
    );
    assert_eq!(
        settings_auth_key_plan(KeyCode::Enter, false, true, true),
        SettingsAuthKeyPlan::OpenForm
    );
    assert_eq!(
        settings_auth_key_plan(KeyCode::Enter, false, true, false),
        SettingsAuthKeyPlan::Noop
    );
    assert_eq!(
        settings_auth_key_plan(KeyCode::Char('s'), false, true, false),
        SettingsAuthKeyPlan::Save
    );
    assert_eq!(
        settings_auth_key_plan(KeyCode::Char('d'), false, true, true),
        SettingsAuthKeyPlan::Noop
    );
}

#[test]
fn settings_env_header_key_plan_routes_role_header_arrows() {
    let collapsed = SettingsEnvRow::RoleHeader {
        role: "ops".to_owned(),
        expanded: false,
    };
    let expanded = SettingsEnvRow::RoleHeader {
        role: "ops".to_owned(),
        expanded: true,
    };
    let key_row = SettingsEnvRow::Key {
        scope: SettingsEnvScope::Global,
        key: "TOKEN".to_owned(),
    };

    assert_eq!(
        settings_env_header_key_plan(KeyCode::Right, SettingsTab::Environments, Some(&collapsed),),
        SettingsEnvHeaderKeyPlan::SetExpanded {
            role: "ops".to_owned(),
            expanded: true,
        }
    );
    assert_eq!(
        settings_env_header_key_plan(KeyCode::Left, SettingsTab::Environments, Some(&expanded)),
        SettingsEnvHeaderKeyPlan::SetExpanded {
            role: "ops".to_owned(),
            expanded: false,
        }
    );
    assert_eq!(
        settings_env_header_key_plan(KeyCode::Right, SettingsTab::Environments, Some(&expanded)),
        SettingsEnvHeaderKeyPlan::Consume
    );
    assert_eq!(
        settings_env_header_key_plan(KeyCode::Left, SettingsTab::Environments, Some(&key_row)),
        SettingsEnvHeaderKeyPlan::Consume
    );
    assert_eq!(
        settings_env_header_key_plan(KeyCode::Right, SettingsTab::General, Some(&collapsed)),
        SettingsEnvHeaderKeyPlan::Continue
    );
    assert_eq!(
        settings_env_header_key_plan(KeyCode::Enter, SettingsTab::Environments, Some(&collapsed)),
        SettingsEnvHeaderKeyPlan::Continue
    );
}

#[test]
fn settings_env_selected_header_key_plan_uses_current_flat_selection() {
    let pending = env_config();
    let expanded = BTreeSet::new();
    let rows = settings_env_flat_rows(&pending, &expanded);
    let selected = rows
        .iter()
        .position(|row| {
            matches!(
                row,
                SettingsEnvRow::RoleHeader {
                    role,
                    expanded: false,
                } if role == "alpha"
            )
        })
        .unwrap_or(usize::MAX);

    assert_eq!(
        settings_env_selected_header_key_plan(
            KeyCode::Right,
            SettingsTab::Environments,
            &pending,
            &expanded,
            selected,
        ),
        SettingsEnvHeaderKeyPlan::SetExpanded {
            role: "alpha".to_owned(),
            expanded: true,
        }
    );
}

#[test]
fn settings_top_level_key_plan_applies_shell_before_header_and_delegates() {
    let pending = env_config();
    let expanded = BTreeSet::new();
    let rows = settings_env_flat_rows(&pending, &expanded);
    let role_header = rows
        .iter()
        .position(|row| {
            matches!(
                row,
                SettingsEnvRow::RoleHeader {
                    role,
                    expanded: false,
                } if role == "alpha"
            )
        })
        .unwrap_or(usize::MAX);

    assert_eq!(
        settings_top_level_key_plan(
            KeyCode::Tab,
            SettingsTab::Environments,
            false,
            false,
            &pending,
            &expanded,
            role_header,
        ),
        SettingsTopLevelKeyPlan::MoveTab {
            delta: 1,
            focus_tab_bar: true,
        }
    );
    assert_eq!(
        settings_top_level_key_plan(
            KeyCode::Right,
            SettingsTab::Environments,
            false,
            false,
            &pending,
            &expanded,
            role_header,
        ),
        SettingsTopLevelKeyPlan::SetEnvRoleExpanded {
            role: "alpha".to_owned(),
            expanded: true,
        }
    );
    assert_eq!(
        settings_top_level_key_plan(
            KeyCode::Char('s'),
            SettingsTab::Trust,
            false,
            false,
            &pending,
            &expanded,
            role_header,
        ),
        SettingsTopLevelKeyPlan::Delegate(SettingsTab::Trust)
    );
}
