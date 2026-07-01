//! Environment helpers: settings_env_flat_rows, set_settings_env_value, toggle_*, remove_*, step_cursor_*, settings_env_change_count, etc.

use super::*;

pub fn set_role_expanded(expanded_roles: &mut BTreeSet<String>, role: String, expanded: bool) {
    if expanded {
        expanded_roles.insert(role);
    } else {
        expanded_roles.remove(&role);
    }
}

#[must_use]
pub fn settings_env_flat_rows<V>(
    pending: &SettingsEnvConfig<V>,
    expanded_roles: &BTreeSet<String>,
) -> Vec<SettingsEnvRow> {
    let mut rows = Vec::new();
    for key in pending.env.keys() {
        rows.push(SettingsEnvRow::Key {
            scope: SettingsEnvScope::Global,
            key: key.clone(),
        });
    }
    if !pending.env.is_empty() {
        rows.push(SettingsEnvRow::SectionSpacer);
    }
    rows.push(SettingsEnvRow::GlobalAddSentinel);
    for (role, role_env) in &pending.roles {
        if role_env.is_empty() {
            continue;
        }
        rows.push(SettingsEnvRow::SectionSpacer);
        let expanded = expanded_roles.contains(role);
        rows.push(SettingsEnvRow::RoleHeader {
            role: role.clone(),
            expanded,
        });
        if expanded {
            for key in role_env.keys() {
                rows.push(SettingsEnvRow::Key {
                    scope: SettingsEnvScope::Role(role.clone()),
                    key: key.clone(),
                });
            }
            rows.push(SettingsEnvRow::SectionSpacer);
            rows.push(SettingsEnvRow::RoleAddSentinel(role.clone()));
        }
    }
    rows
}

#[must_use]
pub fn settings_env_flat_row_count<V>(
    pending: &SettingsEnvConfig<V>,
    expanded_roles: &BTreeSet<String>,
) -> usize {
    settings_env_flat_rows(pending, expanded_roles).len()
}

#[must_use]
pub fn settings_env_value<'a, V>(
    pending: &'a SettingsEnvConfig<V>,
    scope: &SettingsEnvScope,
    key: &str,
) -> Option<&'a V> {
    match scope {
        SettingsEnvScope::Global => pending.env.get(key),
        SettingsEnvScope::Role(role) => pending
            .roles
            .get(role)
            .and_then(|role_env| role_env.get(key)),
    }
}

#[must_use]
pub fn forbidden_settings_env_keys<V>(
    pending: &SettingsEnvConfig<V>,
    scope: &SettingsEnvScope,
) -> Vec<String> {
    match scope {
        SettingsEnvScope::Global => pending.env.keys().cloned().collect(),
        SettingsEnvScope::Role(role) => pending
            .roles
            .get(role)
            .map(|role_env| role_env.keys().cloned().collect())
            .unwrap_or_default(),
    }
}

pub fn set_settings_env_value<V>(
    pending: &mut SettingsEnvConfig<V>,
    expanded_roles: &mut BTreeSet<String>,
    scope: &SettingsEnvScope,
    key: &str,
    value: V,
) {
    match scope {
        SettingsEnvScope::Global => {
            pending.env.insert(key.to_owned(), value);
        }
        SettingsEnvScope::Role(role) => {
            pending
                .roles
                .entry(role.clone())
                .or_default()
                .insert(key.to_owned(), value);
            expanded_roles.insert(role.clone());
        }
    }
}

pub fn toggle_settings_env_mask_for_row<V>(
    unmasked_rows: &mut BTreeSet<(SettingsEnvScope, String)>,
    pending: &SettingsEnvConfig<V>,
    row: Option<&SettingsEnvRow>,
    is_maskable: impl FnOnce(&V) -> bool,
) -> bool {
    let Some(SettingsEnvRow::Key { scope, key }) = row else {
        return false;
    };
    let Some(value) = settings_env_value(pending, scope, key) else {
        return false;
    };
    if !is_maskable(value) {
        return false;
    }

    let tag = (scope.clone(), key.clone());
    if !unmasked_rows.remove(&tag) {
        unmasked_rows.insert(tag);
    }
    true
}

pub fn toggle_selected_settings_env_mask<V>(
    unmasked_rows: &mut BTreeSet<(SettingsEnvScope, String)>,
    pending: &SettingsEnvConfig<V>,
    expanded_roles: &BTreeSet<String>,
    selected: usize,
    is_maskable: impl FnOnce(&V) -> bool,
) -> bool {
    let rows = settings_env_flat_rows(pending, expanded_roles);
    toggle_settings_env_mask_for_row(unmasked_rows, pending, rows.get(selected), is_maskable)
}

pub fn toggle_selected_settings_env_maskable_value(
    unmasked_rows: &mut BTreeSet<(SettingsEnvScope, String)>,
    pending: &SettingsEnvConfig<EnvValue>,
    expanded_roles: &BTreeSet<String>,
    selected: usize,
) -> bool {
    toggle_selected_settings_env_mask(unmasked_rows, pending, expanded_roles, selected, |value| {
        !matches!(value, EnvValue::OpRef(_))
    })
}

pub fn remove_settings_env_row<V>(
    pending: &mut SettingsEnvConfig<V>,
    expanded_roles: &BTreeSet<String>,
    selected: &mut usize,
    row: Option<&SettingsEnvRow>,
) -> bool {
    let Some(SettingsEnvRow::Key { scope, key }) = row else {
        return false;
    };
    match scope {
        SettingsEnvScope::Global => {
            pending.env.remove(key);
        }
        SettingsEnvScope::Role(role) => {
            if let Some(role_env) = pending.roles.get_mut(role) {
                role_env.remove(key);
            }
        }
    }
    let row_count = settings_env_flat_row_count(pending, expanded_roles);
    *selected = (*selected).min(row_count.saturating_sub(1));
    true
}

pub fn remove_selected_settings_env_row<V>(
    pending: &mut SettingsEnvConfig<V>,
    expanded_roles: &BTreeSet<String>,
    selected: &mut usize,
) -> bool {
    let rows = settings_env_flat_rows(pending, expanded_roles);
    remove_settings_env_row(pending, expanded_roles, selected, rows.get(*selected))
}

#[must_use]
pub fn settings_env_add_target_for_row(row: Option<&SettingsEnvRow>) -> Option<SettingsEnvScope> {
    match row? {
        SettingsEnvRow::Key {
            scope: SettingsEnvScope::Global,
            ..
        }
        | SettingsEnvRow::GlobalAddSentinel => Some(SettingsEnvScope::Global),
        SettingsEnvRow::RoleHeader { role, .. }
        | SettingsEnvRow::Key {
            scope: SettingsEnvScope::Role(role),
            ..
        }
        | SettingsEnvRow::RoleAddSentinel(role) => Some(SettingsEnvScope::Role(role.clone())),
        SettingsEnvRow::SectionSpacer => None,
    }
}

#[must_use]
pub fn settings_env_selected_add_target<V>(
    pending: &SettingsEnvConfig<V>,
    expanded_roles: &BTreeSet<String>,
    selected: usize,
) -> Option<SettingsEnvScope> {
    let rows = settings_env_flat_rows(pending, expanded_roles);
    settings_env_add_target_for_row(rows.get(selected))
}

#[must_use]
pub fn settings_env_picker_target_for_row(
    row: Option<&SettingsEnvRow>,
) -> Option<(SettingsEnvScope, Option<String>)> {
    match row? {
        SettingsEnvRow::Key { scope, key } => Some((scope.clone(), Some(key.clone()))),
        SettingsEnvRow::GlobalAddSentinel => Some((SettingsEnvScope::Global, None)),
        SettingsEnvRow::RoleAddSentinel(role) => Some((SettingsEnvScope::Role(role.clone()), None)),
        SettingsEnvRow::RoleHeader { .. } | SettingsEnvRow::SectionSpacer => None,
    }
}

#[must_use]
pub fn settings_env_selected_picker_target<V>(
    pending: &SettingsEnvConfig<V>,
    expanded_roles: &BTreeSet<String>,
    selected: usize,
) -> Option<(SettingsEnvScope, Option<String>)> {
    let rows = settings_env_flat_rows(pending, expanded_roles);
    settings_env_picker_target_for_row(rows.get(selected))
}

#[must_use]
pub fn settings_env_enter_plan_for_row<V>(
    pending: &SettingsEnvConfig<V>,
    row: Option<&SettingsEnvRow>,
    can_edit_value: impl FnOnce(Option<&V>) -> bool,
) -> SettingsEnvEnterPlan {
    match row {
        Some(SettingsEnvRow::Key { scope, key }) => {
            let value = settings_env_value(pending, scope, key);
            if can_edit_value(value) {
                SettingsEnvEnterPlan::EditValue {
                    scope: scope.clone(),
                    key: key.clone(),
                }
            } else {
                SettingsEnvEnterPlan::Noop
            }
        }
        Some(SettingsEnvRow::GlobalAddSentinel) => SettingsEnvEnterPlan::OpenScopePicker,
        Some(SettingsEnvRow::RoleHeader {
            role,
            expanded: false,
        }) => SettingsEnvEnterPlan::ExpandRole(role.clone()),
        Some(SettingsEnvRow::RoleAddSentinel(role)) => SettingsEnvEnterPlan::AddRoleKey {
            scope: SettingsEnvScope::Role(role.clone()),
        },
        Some(SettingsEnvRow::RoleHeader { .. } | SettingsEnvRow::SectionSpacer) | None => {
            SettingsEnvEnterPlan::Noop
        }
    }
}

#[must_use]
pub fn settings_env_selected_enter_plan(
    pending: &SettingsEnvConfig<EnvValue>,
    expanded_roles: &BTreeSet<String>,
    selected: usize,
) -> SettingsEnvEnterPlan {
    let rows = settings_env_flat_rows(pending, expanded_roles);
    settings_env_enter_plan_for_row(pending, rows.get(selected), |value| {
        !value.is_some_and(|v| matches!(v, EnvValue::OpRef(_)))
    })
}

#[must_use]
pub fn step_cursor_down_by<F>(candidate: usize, max: usize, mut is_skipped: F) -> usize
where
    F: FnMut(usize) -> bool,
{
    let mut idx = candidate;
    while idx <= max {
        if is_skipped(idx) {
            idx += 1;
        } else {
            return idx;
        }
    }
    candidate
}

#[must_use]
pub fn step_cursor_up_by<F>(candidate: usize, mut is_skipped: F) -> usize
where
    F: FnMut(usize) -> bool,
{
    let mut idx = candidate;
    loop {
        if is_skipped(idx) {
            if idx == 0 {
                return 0;
            }
            idx -= 1;
        } else {
            return idx;
        }
    }
}

#[must_use]
pub fn settings_vec_change_count<T: PartialEq>(original: &[T], pending: &[T]) -> usize {
    let common_changes = original
        .iter()
        .zip(pending.iter())
        .filter(|(a, b)| a != b)
        .count();
    common_changes + original.len().abs_diff(pending.len())
}

#[must_use]
pub fn settings_map_change_count<V: PartialEq>(
    original: &BTreeMap<String, V>,
    pending: &BTreeMap<String, V>,
) -> usize {
    let mut count = 0;
    for (key, value) in pending {
        match original.get(key) {
            None => count += 1,
            Some(original_value) if original_value != value => count += 1,
            _ => {}
        }
    }
    for key in original.keys() {
        if !pending.contains_key(key) {
            count += 1;
        }
    }
    count
}
