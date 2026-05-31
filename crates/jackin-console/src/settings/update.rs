use std::collections::{BTreeMap, BTreeSet};

use super::state::{
    SettingsEnvConfig, SettingsEnvRow, SettingsEnvScope, SettingsGeneralState, SettingsTab,
    SettingsTrustState,
};

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

pub fn move_general_selection(state: &mut SettingsGeneralState, delta: isize) {
    state.selected = crate::focus::moved_selection(state.selected, 2, delta);
}

pub fn toggle_general_selected(state: &mut SettingsGeneralState) {
    match state.selected {
        0 => {
            state.pending_coauthor_trailer = !state.pending_coauthor_trailer;
        }
        1 => {
            state.pending_dco = !state.pending_dco;
        }
        _ => {}
    }
}

pub fn move_trust_selection(state: &mut SettingsTrustState, delta: isize) {
    state.selected = crate::focus::moved_selection(state.selected, state.pending.len(), delta);
}

pub fn toggle_trust_selected(state: &mut SettingsTrustState) {
    if let Some(row) = state.pending.get_mut(state.selected) {
        row.trusted = !row.trusted;
    }
}

pub fn set_role_expanded(expanded_roles: &mut BTreeSet<String>, role: String, expanded: bool) {
    if expanded {
        expanded_roles.insert(role);
    } else {
        expanded_roles.remove(&role);
    }
}

pub fn toggle_readonly(readonly: &mut bool) {
    *readonly = !*readonly;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn env_config() -> SettingsEnvConfig<&'static str> {
        SettingsEnvConfig {
            env: BTreeMap::from([("GLOBAL".to_string(), "x")]),
            roles: BTreeMap::from([
                (
                    "alpha".to_string(),
                    BTreeMap::from([("ROLE_A".to_string(), "x"), ("ROLE_B".to_string(), "x")]),
                ),
                ("empty".to_string(), BTreeMap::new()),
            ]),
        }
    }

    #[test]
    fn settings_env_flat_rows_include_expanded_role_entries() {
        let expanded = BTreeSet::from(["alpha".to_string()]);
        let rows = settings_env_flat_rows(&env_config(), &expanded);
        assert!(matches!(rows[0], SettingsEnvRow::Key { .. }));
        assert!(matches!(rows[1], SettingsEnvRow::SectionSpacer));
        assert!(matches!(rows[2], SettingsEnvRow::GlobalAddSentinel));
        assert!(
            rows.iter()
                .any(|row| matches!(row, SettingsEnvRow::RoleHeader { role, expanded: true } if role == "alpha"))
        );
        assert!(
            rows.iter()
                .any(|row| matches!(row, SettingsEnvRow::RoleAddSentinel(role) if role == "alpha"))
        );
        assert!(
            !rows.iter().any(
                |row| matches!(row, SettingsEnvRow::RoleHeader { role, .. } if role == "empty")
            )
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
}
