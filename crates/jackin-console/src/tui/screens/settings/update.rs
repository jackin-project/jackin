use std::collections::{BTreeMap, BTreeSet};

use super::model::{
    GlobalMountConfirm, SettingsEnvConfig, SettingsEnvEnterPlan, SettingsEnvRow,
    SettingsEnvScope, SettingsGeneralState, SettingsTab, SettingsTrustState,
};
use jackin_tui::ModalOutcome;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsConfirmPlan {
    Continue,
    Commit,
    Cancel { abort_sensitive: bool },
}

#[must_use]
pub const fn settings_confirm_plan(
    action: GlobalMountConfirm,
    outcome: ModalOutcome<bool>,
) -> SettingsConfirmPlan {
    match outcome {
        ModalOutcome::Commit(true) => SettingsConfirmPlan::Commit,
        ModalOutcome::Commit(false) | ModalOutcome::Cancel => SettingsConfirmPlan::Cancel {
            abort_sensitive: matches!(action, GlobalMountConfirm::Sensitive),
        },
        ModalOutcome::Continue => SettingsConfirmPlan::Continue,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsAuthKindPlan<K> {
    pub selected_kind: Option<K>,
    pub selected: usize,
}

#[must_use]
pub const fn clear_settings_auth_kind_plan<K>() -> SettingsAuthKindPlan<K> {
    SettingsAuthKindPlan {
        selected_kind: None,
        selected: 0,
    }
}

#[must_use]
pub fn enter_settings_auth_kind_plan<K>(
    selected_kind: Option<K>,
) -> Option<SettingsAuthKindPlan<K>> {
    match selected_kind {
        Some(kind) => Some(SettingsAuthKindPlan {
            selected_kind: Some(kind),
            selected: 0,
        }),
        None => None,
    }
}

pub fn move_general_selection(state: &mut SettingsGeneralState, delta: isize) {
    state.selected = crate::focus::moved_selection(state.selected, 2, delta);
}

#[must_use]
pub fn settings_auth_selection_plan(selected: usize, row_count: usize, delta: isize) -> usize {
    crate::focus::moved_selection(selected, row_count, delta)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsSelectionScrollPlan {
    pub selected: usize,
    pub scroll_y: u16,
}

#[must_use]
pub fn settings_trust_selection_plan(
    selected: usize,
    row_count: usize,
    delta: isize,
    current_scroll_y: u16,
    term_height: u16,
    footer_h: u16,
) -> SettingsSelectionScrollPlan {
    let selected = crate::focus::moved_selection(selected, row_count, delta);
    SettingsSelectionScrollPlan {
        selected,
        scroll_y: crate::focus::cursor_scroll_for_panel(
            selected,
            current_scroll_y,
            term_height,
            footer_h,
        ),
    }
}

#[must_use]
pub fn settings_env_selection_plan(
    selected: usize,
    rows: &[SettingsEnvRow],
    delta: isize,
    current_scroll_y: u16,
    term_height: u16,
    footer_h: u16,
) -> SettingsSelectionScrollPlan {
    let max = rows.len().saturating_sub(1);
    let candidate = if delta.is_negative() {
        selected.saturating_sub(delta.unsigned_abs())
    } else {
        selected.saturating_add(delta as usize).min(max)
    };
    let selected = if delta.is_negative() {
        step_cursor_up_by(candidate, |idx| {
            matches!(rows.get(idx), Some(SettingsEnvRow::SectionSpacer))
        })
    } else {
        step_cursor_down_by(candidate, max, |idx| {
            matches!(rows.get(idx), Some(SettingsEnvRow::SectionSpacer))
        })
    };
    SettingsSelectionScrollPlan {
        selected,
        scroll_y: crate::focus::cursor_scroll_for_panel(
            selected,
            current_scroll_y,
            term_height,
            footer_h,
        ),
    }
}

#[must_use]
pub fn settings_global_mounts_selection_plan(
    selected: usize,
    mount_count: usize,
    delta: isize,
    current_scroll_y: u16,
    term_height: u16,
    footer_h: u16,
) -> SettingsSelectionScrollPlan {
    let selected = if delta.is_negative() {
        selected.saturating_sub(delta.unsigned_abs())
    } else {
        selected.saturating_add(delta as usize).min(mount_count)
    };
    SettingsSelectionScrollPlan {
        selected,
        scroll_y: crate::focus::cursor_scroll_for_panel(
            selected,
            current_scroll_y,
            term_height,
            footer_h,
        ),
    }
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
) -> Option<SettingsEnvScope> {
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
        Some(SettingsEnvRow::RoleAddSentinel(role)) => {
            SettingsEnvEnterPlan::AddRoleKey {
                scope: SettingsEnvScope::Role(role.clone()),
            }
        }
        Some(SettingsEnvRow::RoleHeader { .. } | SettingsEnvRow::SectionSpacer) | None => {
            SettingsEnvEnterPlan::Noop
        }
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

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum TestAuthKind {
        Claude,
    }

    #[test]
    fn settings_auth_kind_entry_plan_selects_kind_and_resets_row() {
        assert_eq!(
            enter_settings_auth_kind_plan(Some(TestAuthKind::Claude)),
            Some(SettingsAuthKindPlan {
                selected_kind: Some(TestAuthKind::Claude),
                selected: 0,
            })
        );
        assert_eq!(enter_settings_auth_kind_plan::<TestAuthKind>(None), None);
    }

    #[test]
    fn settings_auth_kind_clear_plan_clears_kind_and_resets_row() {
        assert_eq!(
            clear_settings_auth_kind_plan::<TestAuthKind>(),
            SettingsAuthKindPlan {
                selected_kind: None,
                selected: 0,
            }
        );
    }

    #[test]
    fn settings_auth_selection_plan_clamps_to_rows() {
        assert_eq!(settings_auth_selection_plan(0, 3, 99), 2);
        assert_eq!(settings_auth_selection_plan(2, 3, -99), 0);
    }

    #[test]
    fn settings_trust_selection_plan_clamps_and_updates_scroll() {
        let plan = settings_trust_selection_plan(0, 4, 99, 0, 8, 0);
        assert_eq!(plan.selected, 3);
        assert!(plan.scroll_y > 0);
    }

    #[test]
    fn settings_env_selection_plan_skips_spacers_and_updates_scroll() {
        let rows = [
            SettingsEnvRow::Key {
                scope: SettingsEnvScope::Global,
                key: "ALPHA".to_string(),
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
    }

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
            Some(SettingsEnvScope::Global)
        );
        assert_eq!(
            settings_env_add_target_for_row(Some(&role)),
            Some(SettingsEnvScope::Role("alpha".to_string()))
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

    #[test]
    fn settings_env_enter_plan_handles_value_scope_and_headers() {
        let pending = env_config();
        let key = SettingsEnvRow::Key {
            scope: SettingsEnvScope::Global,
            key: "GLOBAL".to_string(),
        };
        let collapsed = SettingsEnvRow::RoleHeader {
            role: "alpha".to_string(),
            expanded: false,
        };
        let expanded = SettingsEnvRow::RoleHeader {
            role: "alpha".to_string(),
            expanded: true,
        };

        assert_eq!(
            settings_env_enter_plan_for_row(&pending, Some(&key), |value| value.is_some()),
            SettingsEnvEnterPlan::EditValue {
                scope: SettingsEnvScope::Global,
                key: "GLOBAL".to_string()
            }
        );
        assert_eq!(
            settings_env_enter_plan_for_row(&pending, Some(&key), |_| false),
            SettingsEnvEnterPlan::Noop
        );
        assert_eq!(
            settings_env_enter_plan_for_row(&pending, Some(&collapsed), |_| true),
            SettingsEnvEnterPlan::ExpandRole("alpha".to_string())
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
            settings_env_enter_plan_for_row(
                &pending,
                Some(&SettingsEnvRow::GlobalAddSentinel),
                |_| true
            ),
            SettingsEnvEnterPlan::OpenScopePicker
        );
        assert_eq!(
            settings_env_enter_plan_for_row(
                &pending,
                Some(&SettingsEnvRow::RoleAddSentinel("alpha".to_string())),
                |_| true
            ),
            SettingsEnvEnterPlan::AddRoleKey {
                scope: SettingsEnvScope::Role("alpha".to_string()),
            }
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
}
