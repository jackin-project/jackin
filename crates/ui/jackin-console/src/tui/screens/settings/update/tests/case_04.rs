// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn settings_trust_selection_plan_clamps_and_updates_scroll() {
    let plan = settings_trust_selection_plan(0, 4, 99, 0, 8, 0);
    assert_eq!(plan.selected, 3);
    assert!(plan.scroll_y > 0);
}

#[test]
fn settings_trust_row_select_plan_bounds_checks_and_focuses_content() {
    assert_eq!(
        settings_trust_row_select_plan(1, 3),
        SettingsTrustRowSelectPlan {
            selected: Some(1),
            content_focused: true,
        }
    );
    assert_eq!(
        settings_trust_row_select_plan(3, 3),
        SettingsTrustRowSelectPlan {
            selected: None,
            content_focused: true,
        }
    );
}

#[test]
fn settings_trust_row_at_position_skips_header_and_applies_scroll() {
    let area = Rect::new(0, 5, 80, 10);

    assert_eq!(settings_trust_row_at_position(area, 1, 7, 0, 3), Some(0));
    assert_eq!(settings_trust_row_at_position(area, 1, 8, 2, 5), Some(3));
    assert_eq!(settings_trust_row_at_position(area, 1, 6, 0, 3), None);
    assert_eq!(settings_trust_row_at_position(area, 1, 10, 0, 3), None);
    assert_eq!(settings_trust_row_at_position(area, 80, 6, 0, 3), None);
}

#[test]
fn settings_trust_hover_target_at_position_maps_trust_rows() {
    let area = Rect::new(0, 5, 80, 10);

    assert_eq!(
        settings_trust_hover_target_at_position(SettingsTab::Trust, false, area, 1, 7, 0, 3),
        Some(SettingsHoverTarget::TrustRow(0))
    );
    assert_eq!(
        settings_trust_hover_target_at_position(SettingsTab::Trust, false, area, 1, 8, 2, 5),
        Some(SettingsHoverTarget::TrustRow(3))
    );
    assert_eq!(
        settings_trust_hover_target_at_position(SettingsTab::Mounts, false, area, 1, 7, 0, 3),
        None
    );
    assert_eq!(
        settings_trust_hover_target_at_position(SettingsTab::Trust, true, area, 1, 7, 0, 3),
        None
    );
}

#[test]
fn settings_trust_clickable_at_position_requires_trust_content_without_modal() {
    let area = Rect::new(0, 5, 80, 10);

    assert!(settings_trust_clickable_at_position(
        SettingsTab::Trust,
        false,
        area,
        1,
        6,
    ));
    assert!(!settings_trust_clickable_at_position(
        SettingsTab::Mounts,
        false,
        area,
        1,
        6,
    ));
    assert!(!settings_trust_clickable_at_position(
        SettingsTab::Trust,
        true,
        area,
        1,
        6,
    ));
    assert!(!settings_trust_clickable_at_position(
        SettingsTab::Trust,
        false,
        area,
        80,
        6,
    ));
}

#[test]
fn settings_scroll_focus_plan_routes_by_tab_and_modal() {
    assert_eq!(
        settings_scroll_focus_plan(SettingsTab::Mounts, false, true),
        SettingsScrollFocusPlan {
            mounts: true,
            env: false,
            auth: false,
            trust: false,
        }
    );
    assert_eq!(
        settings_scroll_focus_plan(SettingsTab::Auth, false, true),
        SettingsScrollFocusPlan {
            mounts: false,
            env: false,
            auth: true,
            trust: false,
        }
    );
    assert_eq!(
        settings_scroll_focus_plan(SettingsTab::Trust, true, true),
        SettingsScrollFocusPlan {
            mounts: false,
            env: false,
            auth: false,
            trust: false,
        }
    );
}

#[test]
fn settings_modal_open_reports_any_modal_surface() {
    assert!(!settings_modal_open(false, false, false, false));
    assert!(settings_modal_open(true, false, false, false));
    assert!(settings_modal_open(false, true, false, false));
    assert!(settings_modal_open(false, false, true, false));
    assert!(settings_modal_open(false, false, false, true));
}

#[test]
fn settings_horizontal_scroll_plan_updates_and_clamps_offset() {
    assert_eq!(settings_horizontal_scroll_plan(0, 8, 10, 40), 8);
    assert_eq!(settings_horizontal_scroll_plan(8, -99, 10, 40), 0);
}

#[test]
fn settings_env_selection_plan_skips_spacers_and_updates_scroll() {
    let rows = [
        SettingsEnvRow::Key {
            scope: SettingsEnvScope::Global,
            key: "ALPHA".to_owned(),
        },
        SettingsEnvRow::SectionSpacer,
        SettingsEnvRow::GlobalAddSentinel,
    ];
    let plan = settings_env_selection_plan(0, &rows, 1, 0, 8, 0);
    assert_eq!(plan.selected, 2);
    assert!(plan.scroll_y > 0);
}

#[test]
fn settings_global_mounts_selection_plan_clamps_to_add_row() {
    let plan = settings_global_mounts_selection_plan(0, 2, 99, 0, 8, 0);
    assert_eq!(plan.selected, 2);
    assert!(plan.scroll_y > 0);
    assert_eq!(settings_global_mounts_selected_index(99, 2), 2);
    assert!(settings_global_mounts_add_row_selected(2, 2));
    assert!(!settings_global_mounts_add_row_selected(1, 2));
    assert_eq!(settings_global_mounts_added_index(3), 2);
    assert_eq!(settings_global_mounts_added_index(0), 0);
    assert_eq!(settings_auth_selected_index(99, 2), 1);
    assert_eq!(settings_auth_selected_index(99, 0), 0);
}

#[test]
fn settings_env_flat_rows_include_expanded_role_entries() {
    let expanded = BTreeSet::from(["alpha".to_owned()]);
    let rows = settings_env_flat_rows(&env_config(), &expanded);
    assert!(matches!(rows[0], SettingsEnvRow::Key { .. }));
    assert!(matches!(rows[1], SettingsEnvRow::SectionSpacer));
    assert!(matches!(rows[2], SettingsEnvRow::GlobalAddSentinel));
    assert!(rows.iter().any(
        |row| matches!(row, SettingsEnvRow::RoleHeader { role, expanded: true } if role == "alpha")
    ));
    assert!(
        rows.iter()
            .any(|row| matches!(row, SettingsEnvRow::RoleAddSentinel(role) if role == "alpha"))
    );
    assert!(
        !rows
            .iter()
            .any(|row| matches!(row, SettingsEnvRow::RoleHeader { role, .. } if role == "empty"))
    );
}

#[test]
fn settings_env_flat_rows_collapse_role_entries() {
    let rows = settings_env_flat_rows(&env_config(), &BTreeSet::new());
    assert!(rows.iter().any(
        |row| matches!(row, SettingsEnvRow::RoleHeader { role, expanded: false } if role == "alpha")
    ));
    assert!(
        !rows
            .iter()
            .any(|row| matches!(row, SettingsEnvRow::RoleAddSentinel(role) if role == "alpha"))
    );
}

#[test]
fn settings_env_value_and_forbidden_keys_follow_scope() {
    let pending = env_config();

    assert_eq!(
        settings_env_value(&pending, &SettingsEnvScope::Global, "GLOBAL"),
        Some(&"x")
    );
    assert_eq!(
        settings_env_value(&pending, &SettingsEnvScope::Role("alpha".into()), "ROLE_A"),
        Some(&"x")
    );
    assert_eq!(
        forbidden_settings_env_keys(&pending, &SettingsEnvScope::Role("alpha".into())),
        vec!["ROLE_A".to_owned(), "ROLE_B".to_owned()]
    );
}

#[test]
fn settings_env_selected_key_matches_checks_selected_key_value() {
    let pending = env_config();
    let rows = settings_env_flat_rows(&pending, &BTreeSet::from(["alpha".to_owned()]));
    let role_b = rows
        .iter()
        .position(|row| {
            matches!(
                row,
                SettingsEnvRow::Key {
                    scope: SettingsEnvScope::Role(role),
                    key,
                } if role == "alpha" && key == "ROLE_B"
            )
        })
        .unwrap();

    assert!(settings_env_selected_key_matches(
        &pending,
        &rows,
        role_b,
        |value| *value == "x"
    ));
    assert!(!settings_env_selected_key_matches(
        &pending,
        &rows,
        role_b,
        |value| *value == "missing"
    ));
    assert!(!settings_env_selected_key_matches(
        &pending,
        &rows,
        usize::MAX,
        |_| true
    ));
}

#[test]
fn settings_env_selected_key_is_op_ref_checks_selected_value_shape() {
    let pending = SettingsEnvConfig {
        env: BTreeMap::from([(
            "GLOBAL".to_owned(),
            EnvValue::OpRef(jackin_core::OpRef {
                op: "op://vault/item/password".to_owned(),
                path: "Vault/Item/password".to_owned(),
                account: None,
                on_demand: false,
            }),
        )]),
        roles: BTreeMap::new(),
    };
    let rows = settings_env_flat_rows(&pending, &BTreeSet::new());

    assert!(settings_env_selected_key_is_op_ref(&pending, &rows, 0));
    assert!(!settings_env_selected_key_is_op_ref(
        &pending,
        &rows,
        usize::MAX,
    ));
}

#[test]
fn settings_env_selected_is_op_ref_builds_current_rows() {
    let pending = SettingsEnvConfig {
        env: BTreeMap::from([(
            "GLOBAL".to_owned(),
            EnvValue::OpRef(jackin_core::OpRef {
                op: "op://vault/item/password".to_owned(),
                path: "Vault/Item/password".to_owned(),
                account: None,
                on_demand: false,
            }),
        )]),
        roles: BTreeMap::new(),
    };

    assert!(settings_env_selected_is_op_ref(
        &pending,
        &BTreeSet::new(),
        0
    ));
    assert!(!settings_env_selected_is_op_ref(
        &pending,
        &BTreeSet::new(),
        usize::MAX,
    ));
}

#[test]
fn settings_env_delete_key_for_row_extracts_key_rows_only() {
    let key_row = SettingsEnvRow::Key {
        scope: SettingsEnvScope::Global,
        key: "TOKEN".to_owned(),
    };
    let header = SettingsEnvRow::RoleHeader {
        role: "ops".to_owned(),
        expanded: true,
    };

    assert_eq!(
        settings_env_delete_key_for_row(Some(&key_row)),
        Some("TOKEN")
    );
    assert_eq!(settings_env_delete_key_for_row(Some(&header)), None);
    assert_eq!(settings_env_delete_key_for_row(None), None);
}

#[test]
fn settings_env_selected_delete_key_extracts_current_selected_key() {
    let pending = env_config();
    let expanded = BTreeSet::from(["alpha".to_owned()]);
    let rows = settings_env_flat_rows(&pending, &expanded);
    let selected = rows
        .iter()
        .position(|row| {
            matches!(
                row,
                SettingsEnvRow::Key {
                    scope: SettingsEnvScope::Role(role),
                    key,
                } if role == "alpha" && key == "ROLE_B"
            )
        })
        .unwrap_or(usize::MAX);

    assert_eq!(
        settings_env_selected_delete_key(&pending, &expanded, selected),
        Some("ROLE_B".to_owned())
    );
}

#[test]
fn set_settings_env_value_expands_role_scope() {
    let mut pending = SettingsEnvConfig {
        env: BTreeMap::new(),
        roles: BTreeMap::new(),
    };
    let mut expanded = BTreeSet::new();

    set_settings_env_value(
        &mut pending,
        &mut expanded,
        &SettingsEnvScope::Role("alpha".into()),
        "TOKEN",
        "secret",
    );

    assert_eq!(
        settings_env_value(&pending, &SettingsEnvScope::Role("alpha".into()), "TOKEN"),
        Some(&"secret")
    );
    assert!(expanded.contains("alpha"));
}
