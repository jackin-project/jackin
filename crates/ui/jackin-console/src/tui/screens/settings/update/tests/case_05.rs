// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn toggle_settings_env_mask_for_row_skips_unmaskable_values() {
    let pending = env_config();
    let mut unmasked = BTreeSet::new();
    let row = SettingsEnvRow::Key {
        scope: SettingsEnvScope::Global,
        key: "GLOBAL".to_owned(),
    };

    assert!(!toggle_settings_env_mask_for_row(
        &mut unmasked,
        &pending,
        Some(&row),
        |_| false
    ));
    assert!(unmasked.is_empty());

    assert!(toggle_settings_env_mask_for_row(
        &mut unmasked,
        &pending,
        Some(&row),
        |_| true
    ));
    assert!(unmasked.contains(&(SettingsEnvScope::Global, "GLOBAL".to_owned())));
}

#[test]
fn toggle_selected_settings_env_mask_uses_current_flat_selection() {
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
                } if role == "alpha" && key == "ROLE_A"
            )
        })
        .unwrap_or(usize::MAX);
    let mut unmasked = BTreeSet::new();

    assert!(toggle_selected_settings_env_mask(
        &mut unmasked,
        &pending,
        &expanded,
        selected,
        |_| true
    ));

    assert!(unmasked.contains(&(
        SettingsEnvScope::Role("alpha".to_owned()),
        "ROLE_A".to_owned()
    )));
}

#[test]
fn toggle_selected_settings_env_maskable_value_skips_op_refs() {
    let pending = SettingsEnvConfig {
        env: BTreeMap::from([
            ("PLAIN".to_owned(), EnvValue::Plain("value".to_owned())),
            (
                "SECRET".to_owned(),
                EnvValue::OpRef(jackin_core::OpRef {
                    op: "op://vault/item/password".to_owned(),
                    path: "Vault/Item/password".to_owned(),
                    account: None,
                    on_demand: false,
                }),
            ),
        ]),
        roles: BTreeMap::new(),
    };
    let expanded = BTreeSet::new();
    let rows = settings_env_flat_rows(&pending, &expanded);
    let plain = rows
        .iter()
        .position(|row| matches!(row, SettingsEnvRow::Key { key, .. } if key == "PLAIN"))
        .unwrap_or(usize::MAX);
    let secret = rows
        .iter()
        .position(|row| matches!(row, SettingsEnvRow::Key { key, .. } if key == "SECRET"))
        .unwrap_or(usize::MAX);
    let mut unmasked = BTreeSet::new();

    assert!(toggle_selected_settings_env_maskable_value(
        &mut unmasked,
        &pending,
        &expanded,
        plain,
    ));
    assert!(!toggle_selected_settings_env_maskable_value(
        &mut unmasked,
        &pending,
        &expanded,
        secret,
    ));
}

#[test]
fn remove_settings_env_row_deletes_key_and_clamps_selection() {
    let mut pending = env_config();
    let expanded = BTreeSet::from(["alpha".to_owned()]);
    let mut selected = 99;
    let row = SettingsEnvRow::Key {
        scope: SettingsEnvScope::Role("alpha".to_owned()),
        key: "ROLE_B".to_owned(),
    };

    assert!(remove_settings_env_row(
        &mut pending,
        &expanded,
        &mut selected,
        Some(&row),
    ));

    assert!(!pending.roles["alpha"].contains_key("ROLE_B"));
    assert_eq!(
        selected,
        settings_env_flat_row_count(&pending, &expanded) - 1
    );
}

#[test]
fn remove_selected_settings_env_row_uses_current_flat_selection() {
    let mut pending = env_config();
    let expanded = BTreeSet::from(["alpha".to_owned()]);
    let rows = settings_env_flat_rows(&pending, &expanded);
    let mut selected = rows
        .iter()
        .position(|row| {
            matches!(
                row,
                SettingsEnvRow::Key {
                    scope: SettingsEnvScope::Role(role),
                    key,
                } if role == "alpha" && key == "ROLE_A"
            )
        })
        .unwrap_or(usize::MAX);

    assert!(remove_selected_settings_env_row(
        &mut pending,
        &expanded,
        &mut selected,
    ));

    assert!(!pending.roles["alpha"].contains_key("ROLE_A"));
    assert!(selected < settings_env_flat_row_count(&pending, &expanded));
}

#[test]
fn settings_env_add_target_follows_row_scope() {
    let global = SettingsEnvRow::GlobalAddSentinel;
    let role = SettingsEnvRow::Key {
        scope: SettingsEnvScope::Role("alpha".to_owned()),
        key: "TOKEN".to_owned(),
    };

    assert_eq!(
        settings_env_add_target_for_row(Some(&global)),
        Some(SettingsEnvScope::Global)
    );
    assert_eq!(
        settings_env_add_target_for_row(Some(&role)),
        Some(SettingsEnvScope::Role("alpha".to_owned()))
    );
}

#[test]
fn settings_env_selected_add_target_uses_current_flat_selection() {
    let pending = env_config();
    let expanded = BTreeSet::from(["alpha".to_owned()]);
    let rows = settings_env_flat_rows(&pending, &expanded);
    let selected = rows
        .iter()
        .position(|row| matches!(row, SettingsEnvRow::RoleAddSentinel(role) if role == "alpha"))
        .unwrap_or(usize::MAX);

    assert_eq!(
        settings_env_selected_add_target(&pending, &expanded, selected),
        Some(SettingsEnvScope::Role("alpha".to_owned()))
    );
}

#[test]
fn settings_env_picker_target_skips_headers_and_spacers() {
    let key = SettingsEnvRow::Key {
        scope: SettingsEnvScope::Role("alpha".to_owned()),
        key: "TOKEN".to_owned(),
    };
    let header = SettingsEnvRow::RoleHeader {
        role: "alpha".to_owned(),
        expanded: true,
    };

    assert_eq!(
        settings_env_picker_target_for_row(Some(&key)),
        Some((
            SettingsEnvScope::Role("alpha".to_owned()),
            Some("TOKEN".to_owned())
        ))
    );
    assert_eq!(settings_env_picker_target_for_row(Some(&header)), None);
    assert_eq!(
        settings_env_picker_target_for_row(Some(&SettingsEnvRow::GlobalAddSentinel)),
        Some((SettingsEnvScope::Global, None))
    );
}

#[test]
fn settings_env_selected_picker_target_uses_current_flat_selection() {
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
                } if role == "alpha" && key == "ROLE_A"
            )
        })
        .unwrap_or(usize::MAX);

    assert_eq!(
        settings_env_selected_picker_target(&pending, &expanded, selected),
        Some((
            SettingsEnvScope::Role("alpha".to_owned()),
            Some("ROLE_A".to_owned())
        ))
    );
}

#[test]
fn settings_env_enter_plan_handles_value_scope_and_headers() {
    let pending = env_config();
    let key = SettingsEnvRow::Key {
        scope: SettingsEnvScope::Global,
        key: "GLOBAL".to_owned(),
    };
    let collapsed = SettingsEnvRow::RoleHeader {
        role: "alpha".to_owned(),
        expanded: false,
    };
    let expanded = SettingsEnvRow::RoleHeader {
        role: "alpha".to_owned(),
        expanded: true,
    };

    assert_eq!(
        settings_env_enter_plan_for_row(&pending, Some(&key), |value| value.is_some()),
        SettingsEnvEnterPlan::EditValue {
            scope: SettingsEnvScope::Global,
            key: "GLOBAL".to_owned()
        }
    );
    assert_eq!(
        settings_env_enter_plan_for_row(&pending, Some(&key), |_| false),
        SettingsEnvEnterPlan::Noop
    );
    assert_eq!(
        settings_env_enter_plan_for_row(&pending, Some(&collapsed), |_| true),
        SettingsEnvEnterPlan::ExpandRole("alpha".to_owned())
    );
    assert_eq!(
        settings_env_enter_plan_for_row(&pending, Some(&expanded), |_| true),
        SettingsEnvEnterPlan::Noop
    );
}

#[test]
fn settings_env_enter_plan_handles_add_rows() {
    let pending = env_config();

    assert_eq!(
        settings_env_enter_plan_for_row(&pending, Some(&SettingsEnvRow::GlobalAddSentinel), |_| {
            true
        }),
        SettingsEnvEnterPlan::OpenScopePicker
    );
    assert_eq!(
        settings_env_enter_plan_for_row(
            &pending,
            Some(&SettingsEnvRow::RoleAddSentinel("alpha".to_owned())),
            |_| true
        ),
        SettingsEnvEnterPlan::AddRoleKey {
            scope: SettingsEnvScope::Role("alpha".to_owned()),
        }
    );
}

#[test]
fn settings_env_selected_enter_plan_skips_op_ref_values() {
    let pending = SettingsEnvConfig {
        env: BTreeMap::from([
            ("PLAIN".to_owned(), EnvValue::Plain("value".to_owned())),
            (
                "SECRET".to_owned(),
                EnvValue::OpRef(jackin_core::OpRef {
                    op: "op://vault/item/password".to_owned(),
                    path: "Vault/Item/password".to_owned(),
                    account: None,
                    on_demand: false,
                }),
            ),
        ]),
        roles: BTreeMap::new(),
    };
    let expanded = BTreeSet::new();
    let rows = settings_env_flat_rows(&pending, &expanded);
    let plain = rows
        .iter()
        .position(|row| matches!(row, SettingsEnvRow::Key { key, .. } if key == "PLAIN"))
        .unwrap_or(usize::MAX);
    let secret = rows
        .iter()
        .position(|row| matches!(row, SettingsEnvRow::Key { key, .. } if key == "SECRET"))
        .unwrap_or(usize::MAX);

    assert_eq!(
        settings_env_selected_enter_plan(&pending, &expanded, plain),
        SettingsEnvEnterPlan::EditValue {
            scope: SettingsEnvScope::Global,
            key: "PLAIN".to_owned()
        }
    );
    assert_eq!(
        settings_env_selected_enter_plan(&pending, &expanded, secret),
        SettingsEnvEnterPlan::Noop
    );
}

#[test]
fn settings_confirm_plan_routes_confirm_cancel_and_continue() {
    assert_eq!(
        settings_confirm_plan(GlobalMountConfirm::Save, ModalOutcome::Commit(true)),
        SettingsConfirmPlan::Commit
    );
    assert_eq!(
        settings_confirm_plan(GlobalMountConfirm::Save, ModalOutcome::Commit(false)),
        SettingsConfirmPlan::Cancel {
            abort_sensitive: false
        }
    );
    assert_eq!(
        settings_confirm_plan(GlobalMountConfirm::Sensitive, ModalOutcome::Cancel),
        SettingsConfirmPlan::Cancel {
            abort_sensitive: true
        }
    );
    assert_eq!(
        settings_confirm_plan(GlobalMountConfirm::Remove, ModalOutcome::Continue),
        SettingsConfirmPlan::Continue
    );
}
