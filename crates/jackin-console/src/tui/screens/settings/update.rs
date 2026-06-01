use std::collections::{BTreeMap, BTreeSet};

use super::model::{
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

#[must_use]
pub fn trust_content_width(state: &SettingsTrustState) -> usize {
    state
        .pending
        .iter()
        .map(|row| 42 + jackin_tui::display_cols(&row.git))
        .chain(["  Role                         Trust      Git".len()])
        .max()
        .unwrap_or(0)
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
            pending.env.insert(key.to_string(), value);
        }
        SettingsEnvScope::Role(role) => {
            pending
                .roles
                .entry(role.clone())
                .or_default()
                .insert(key.to_string(), value);
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

#[must_use]
pub fn settings_env_add_target_for_row(
    row: Option<&SettingsEnvRow>,
) -> Option<(SettingsEnvScope, String)> {
    match row? {
        SettingsEnvRow::Key {
            scope: SettingsEnvScope::Global,
            ..
        }
        | SettingsEnvRow::GlobalAddSentinel => {
            Some((SettingsEnvScope::Global, "New global environment key".to_string()))
        }
        SettingsEnvRow::RoleHeader { role, .. }
        | SettingsEnvRow::Key {
            scope: SettingsEnvScope::Role(role),
            ..
        }
        | SettingsEnvRow::RoleAddSentinel(role) => Some((
            SettingsEnvScope::Role(role.clone()),
            format!("New {role} environment key"),
        )),
        SettingsEnvRow::SectionSpacer => None,
    }
}

#[must_use]
pub fn settings_env_picker_target_for_row(
    row: Option<&SettingsEnvRow>,
) -> Option<(SettingsEnvScope, Option<String>)> {
    match row? {
        SettingsEnvRow::Key { scope, key } => Some((scope.clone(), Some(key.clone()))),
        SettingsEnvRow::GlobalAddSentinel => Some((SettingsEnvScope::Global, None)),
        SettingsEnvRow::RoleAddSentinel(role) => {
            Some((SettingsEnvScope::Role(role.clone()), None))
        }
        SettingsEnvRow::RoleHeader { .. } | SettingsEnvRow::SectionSpacer => None,
    }
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
            vec!["ROLE_A".to_string(), "ROLE_B".to_string()]
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

    #[test]
    fn toggle_settings_env_mask_for_row_skips_unmaskable_values() {
        let pending = env_config();
        let mut unmasked = BTreeSet::new();
        let row = SettingsEnvRow::Key {
            scope: SettingsEnvScope::Global,
            key: "GLOBAL".to_string(),
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
        assert!(unmasked.contains(&(SettingsEnvScope::Global, "GLOBAL".to_string())));
    }

    #[test]
    fn remove_settings_env_row_deletes_key_and_clamps_selection() {
        let mut pending = env_config();
        let expanded = BTreeSet::from(["alpha".to_string()]);
        let mut selected = 99;
        let row = SettingsEnvRow::Key {
            scope: SettingsEnvScope::Role("alpha".to_string()),
            key: "ROLE_B".to_string(),
        };

        assert!(remove_settings_env_row(
            &mut pending,
            &expanded,
            &mut selected,
            Some(&row),
        ));

        assert!(!pending.roles["alpha"].contains_key("ROLE_B"));
        assert_eq!(selected, settings_env_flat_row_count(&pending, &expanded) - 1);
    }

    #[test]
    fn settings_env_add_target_follows_row_scope() {
        let global = SettingsEnvRow::GlobalAddSentinel;
        let role = SettingsEnvRow::Key {
            scope: SettingsEnvScope::Role("alpha".to_string()),
            key: "TOKEN".to_string(),
        };

        assert_eq!(
            settings_env_add_target_for_row(Some(&global)),
            Some((
                SettingsEnvScope::Global,
                "New global environment key".to_string()
            ))
        );
        assert_eq!(
            settings_env_add_target_for_row(Some(&role)),
            Some((
                SettingsEnvScope::Role("alpha".to_string()),
                "New alpha environment key".to_string()
            ))
        );
    }

    #[test]
    fn settings_env_picker_target_skips_headers_and_spacers() {
        let key = SettingsEnvRow::Key {
            scope: SettingsEnvScope::Role("alpha".to_string()),
            key: "TOKEN".to_string(),
        };
        let header = SettingsEnvRow::RoleHeader {
            role: "alpha".to_string(),
            expanded: true,
        };

        assert_eq!(
            settings_env_picker_target_for_row(Some(&key)),
            Some((
                SettingsEnvScope::Role("alpha".to_string()),
                Some("TOKEN".to_string())
            ))
        );
        assert_eq!(settings_env_picker_target_for_row(Some(&header)), None);
        assert_eq!(
            settings_env_picker_target_for_row(Some(&SettingsEnvRow::GlobalAddSentinel)),
            Some((SettingsEnvScope::Global, None))
        );
    }
}
