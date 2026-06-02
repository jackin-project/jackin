use std::collections::{BTreeMap, BTreeSet};

use super::model::{AuthRow, EditorTab, SecretsEnterPlan, SecretsRow, SecretsScopeTag};

#[must_use]
pub const fn previous_editor_tab(tab: EditorTab) -> EditorTab {
    match tab {
        EditorTab::General => EditorTab::Auth,
        EditorTab::Mounts => EditorTab::General,
        EditorTab::Roles => EditorTab::Mounts,
        EditorTab::Secrets => EditorTab::Roles,
        EditorTab::Auth => EditorTab::Secrets,
    }
}

#[must_use]
pub const fn next_editor_tab(tab: EditorTab) -> EditorTab {
    match tab {
        EditorTab::General => EditorTab::Mounts,
        EditorTab::Mounts => EditorTab::Roles,
        EditorTab::Roles => EditorTab::Secrets,
        EditorTab::Secrets => EditorTab::Auth,
        EditorTab::Auth => EditorTab::General,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorTabMovePlan {
    pub active_tab: EditorTab,
    pub tab_bar_focused: bool,
    pub active_row: usize,
    pub tab_scroll_x: u16,
    pub tab_scroll_y: u16,
    pub clear_auth_kind: bool,
    pub clear_secret_view_state: bool,
}

#[must_use]
pub const fn editor_tab_move_plan(
    active_tab: EditorTab,
    delta: isize,
    focus_tab_bar: bool,
) -> EditorTabMovePlan {
    let next = if delta.is_negative() {
        previous_editor_tab(active_tab)
    } else {
        next_editor_tab(active_tab)
    };
    EditorTabMovePlan {
        active_tab: next,
        tab_bar_focused: focus_tab_bar,
        active_row: 0,
        tab_scroll_x: 0,
        tab_scroll_y: 0,
        clear_auth_kind: !matches!(next, EditorTab::Auth),
        clear_secret_view_state: matches!(active_tab, EditorTab::Secrets),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorAuthKindPlan<K> {
    pub selected_kind: Option<K>,
    pub active_row: usize,
    pub tab_scroll_x: u16,
    pub tab_scroll_y: u16,
}

#[must_use]
pub const fn clear_editor_auth_kind_plan<K>() -> EditorAuthKindPlan<K> {
    EditorAuthKindPlan {
        selected_kind: None,
        active_row: 0,
        tab_scroll_x: 0,
        tab_scroll_y: 0,
    }
}

#[must_use]
pub fn enter_editor_auth_kind_plan<K>(kind: K) -> EditorAuthKindPlan<K> {
    EditorAuthKindPlan {
        selected_kind: Some(kind),
        active_row: 0,
        tab_scroll_x: 0,
        tab_scroll_y: 0,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorTabSelectPlan {
    pub active_tab: EditorTab,
    pub tab_bar_focused: bool,
    pub active_row: usize,
    pub workspace_mounts_scroll_focused: bool,
    pub clear_auth_kind: bool,
    pub clear_secret_view_state: bool,
}

#[must_use]
pub const fn editor_tab_select_plan(
    previous_tab: EditorTab,
    selected_tab: EditorTab,
) -> EditorTabSelectPlan {
    EditorTabSelectPlan {
        active_tab: selected_tab,
        tab_bar_focused: true,
        active_row: 0,
        workspace_mounts_scroll_focused: false,
        clear_auth_kind: !matches!(selected_tab, EditorTab::Auth),
        clear_secret_view_state: matches!(previous_tab, EditorTab::Secrets)
            && !matches!(selected_tab, EditorTab::Secrets),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorFieldSelectionPlan {
    pub active_row: usize,
    pub tab_scroll_y: u16,
}

#[must_use]
pub fn editor_field_selection_plan(
    active_row: usize,
    delta: isize,
    max_row: usize,
    skipped_rows: &[usize],
    current_scroll_y: u16,
    term_height: u16,
    footer_h: u16,
) -> EditorFieldSelectionPlan {
    let candidate = crate::focus::moved_selection(active_row, max_row.saturating_add(1), delta);
    let next = if delta.is_negative() {
        step_cursor_up(skipped_rows, candidate)
    } else {
        step_cursor_down(skipped_rows, candidate, max_row)
    };
    EditorFieldSelectionPlan {
        active_row: next,
        tab_scroll_y: crate::focus::cursor_scroll_for_panel(
            next,
            current_scroll_y,
            term_height,
            footer_h,
        ),
    }
}

#[must_use]
pub fn step_cursor_down(skipped_rows: &[usize], candidate: usize, max_row: usize) -> usize {
    let mut idx = candidate;
    while idx <= max_row {
        if skipped_rows.contains(&idx) {
            idx += 1;
        } else {
            return idx;
        }
    }
    candidate
}

#[must_use]
pub fn step_cursor_up(skipped_rows: &[usize], candidate: usize) -> usize {
    let mut idx = candidate;
    loop {
        if skipped_rows.contains(&idx) {
            if idx == 0 {
                return 0;
            }
            idx -= 1;
        } else {
            return idx;
        }
    }
}

pub fn toggle_general_selected(
    row: usize,
    keep_awake_enabled: &mut bool,
    git_pull_on_entry: &mut bool,
) {
    match row {
        2 => *keep_awake_enabled = !*keep_awake_enabled,
        3 => *git_pull_on_entry = !*git_pull_on_entry,
        _ => {}
    }
}

pub fn set_role_expanded(expanded_roles: &mut BTreeSet<String>, role: String, expanded: bool) {
    if expanded {
        expanded_roles.insert(role);
    } else {
        expanded_roles.remove(&role);
    }
}

pub fn toggle_mount_readonly(readonly: &mut bool) {
    *readonly = !*readonly;
}

pub fn toggle_allowed_role_at(
    allowed_roles: &mut Vec<String>,
    default_role: &mut Option<String>,
    role_names: &[String],
    index: usize,
) {
    let Some(role) = role_names.get(index) else {
        return;
    };
    let is_all_mode = allowed_roles.is_empty();
    let in_list = allowed_roles.iter().position(|allowed| allowed == role);

    if is_all_mode {
        *allowed_roles = role_names
            .iter()
            .filter(|allowed| allowed.as_str() != role.as_str())
            .cloned()
            .collect();
        if default_role.as_deref() == Some(role.as_str()) {
            *default_role = None;
        }
    } else if let Some(pos) = in_list {
        allowed_roles.remove(pos);
        if default_role.as_deref() == Some(role.as_str()) {
            *default_role = None;
        }
    } else {
        allowed_roles.push(role.clone());
        if allowed_roles.len() == role_names.len()
            && role_names.iter().all(|role| allowed_roles.contains(role))
        {
            allowed_roles.clear();
        }
    }
}

pub fn toggle_default_role_at(
    allowed_roles: &[String],
    default_role: &mut Option<String>,
    role_names: &[String],
    index: usize,
) {
    let Some(role) = role_names.get(index) else {
        return;
    };

    if default_role.as_deref() == Some(role.as_str()) {
        *default_role = None;
        return;
    }

    let role_allowed = allowed_roles.is_empty() || allowed_roles.iter().any(|allowed| allowed == role);
    if role_allowed {
        *default_role = Some(role.clone());
    }
}

pub fn toggle_secret_mask(
    unmasked_rows: &mut BTreeSet<(SecretsScopeTag, String)>,
    scope: SecretsScopeTag,
    key: String,
) {
    let entry = (scope, key);
    if !unmasked_rows.remove(&entry) {
        unmasked_rows.insert(entry);
    }
}

#[must_use]
pub fn secret_delete_target_for_row(
    row: Option<&SecretsRow>,
) -> Option<(SecretsScopeTag, String)> {
    match row? {
        SecretsRow::WorkspaceKeyRow(key) => Some((SecretsScopeTag::Workspace, key.clone())),
        SecretsRow::RoleKeyRow { role, key } => {
            Some((SecretsScopeTag::Role(role.clone()), key.clone()))
        }
        SecretsRow::WorkspaceAddSentinel
        | SecretsRow::RoleHeader { .. }
        | SecretsRow::RoleAddSentinel(_)
        | SecretsRow::SectionSpacer => None,
    }
}

#[must_use]
pub fn secret_unmask_target_for_row(
    row: Option<&SecretsRow>,
    can_unmask_key: impl Fn(&SecretsScopeTag, &str) -> bool,
) -> Option<(SecretsScopeTag, String)> {
    match row? {
        SecretsRow::WorkspaceKeyRow(key) => {
            let scope = SecretsScopeTag::Workspace;
            can_unmask_key(&scope, key).then(|| (scope, key.clone()))
        }
        SecretsRow::RoleKeyRow { role, key } => {
            let scope = SecretsScopeTag::Role(role.clone());
            can_unmask_key(&scope, key).then(|| (scope, key.clone()))
        }
        SecretsRow::WorkspaceAddSentinel
        | SecretsRow::RoleHeader { .. }
        | SecretsRow::RoleAddSentinel(_)
        | SecretsRow::SectionSpacer => None,
    }
}

#[must_use]
pub fn secret_add_target_for_row(row: Option<&SecretsRow>) -> Option<(SecretsScopeTag, String)> {
    match row? {
        SecretsRow::WorkspaceKeyRow(_) | SecretsRow::WorkspaceAddSentinel => Some((
            SecretsScopeTag::Workspace,
            "New workspace environment key".to_string(),
        )),
        SecretsRow::RoleHeader { role, .. }
        | SecretsRow::RoleKeyRow { role, .. }
        | SecretsRow::RoleAddSentinel(role) => Some((
            SecretsScopeTag::Role(role.clone()),
            format!("New {role} environment key"),
        )),
        SecretsRow::SectionSpacer => None,
    }
}

#[must_use]
pub fn secret_picker_target_for_row(
    row: Option<&SecretsRow>,
) -> Option<(SecretsScopeTag, Option<String>)> {
    match row? {
        SecretsRow::WorkspaceKeyRow(key) => Some((SecretsScopeTag::Workspace, Some(key.clone()))),
        SecretsRow::RoleKeyRow { role, key } => {
            Some((SecretsScopeTag::Role(role.clone()), Some(key.clone())))
        }
        SecretsRow::WorkspaceAddSentinel => Some((SecretsScopeTag::Workspace, None)),
        SecretsRow::RoleAddSentinel(role) => Some((SecretsScopeTag::Role(role.clone()), None)),
        SecretsRow::RoleHeader { .. } | SecretsRow::SectionSpacer => None,
    }
}

#[must_use]
pub fn secret_enter_plan_for_row(
    row: Option<&SecretsRow>,
    can_edit_key: impl Fn(&SecretsScopeTag, &str) -> bool,
) -> SecretsEnterPlan {
    match row {
        Some(SecretsRow::WorkspaceKeyRow(key)) => {
            let scope = SecretsScopeTag::Workspace;
            if can_edit_key(&scope, key) {
                SecretsEnterPlan::EditValue {
                    scope,
                    key: key.clone(),
                }
            } else {
                SecretsEnterPlan::Noop
            }
        }
        Some(SecretsRow::WorkspaceAddSentinel) => SecretsEnterPlan::OpenScopePicker,
        Some(SecretsRow::RoleHeader {
            role,
            expanded: false,
        }) => SecretsEnterPlan::ExpandRole(role.clone()),
        Some(SecretsRow::RoleKeyRow { role, key }) => {
            let scope = SecretsScopeTag::Role(role.clone());
            if can_edit_key(&scope, key) {
                SecretsEnterPlan::EditValue {
                    scope,
                    key: key.clone(),
                }
            } else {
                SecretsEnterPlan::Noop
            }
        }
        Some(SecretsRow::RoleAddSentinel(role)) => SecretsEnterPlan::AddRoleKey {
            scope: SecretsScopeTag::Role(role.clone()),
            label: format!("New {role} environment key"),
        },
        Some(SecretsRow::RoleHeader { .. } | SecretsRow::SectionSpacer) | None => {
            SecretsEnterPlan::Noop
        }
    }
}

#[must_use]
pub fn secrets_flat_rows<R, V>(
    workspace_env: &BTreeMap<String, V>,
    roles: &BTreeMap<String, R>,
    expanded_roles: &BTreeSet<String>,
    role_env: impl Fn(&R) -> &BTreeMap<String, V>,
) -> Vec<SecretsRow> {
    let mut rows = Vec::new();
    for key in workspace_env.keys() {
        rows.push(SecretsRow::WorkspaceKeyRow(key.clone()));
    }
    if !workspace_env.is_empty() {
        rows.push(SecretsRow::SectionSpacer);
    }
    rows.push(SecretsRow::WorkspaceAddSentinel);
    for (role, override_) in roles {
        rows.push(SecretsRow::SectionSpacer);
        let expanded = expanded_roles.contains(role);
        rows.push(SecretsRow::RoleHeader {
            role: role.clone(),
            expanded,
        });
        if expanded {
            for key in role_env(override_).keys() {
                rows.push(SecretsRow::RoleKeyRow {
                    role: role.clone(),
                    key: key.clone(),
                });
            }
            rows.push(SecretsRow::SectionSpacer);
            rows.push(SecretsRow::RoleAddSentinel(role.clone()));
        }
    }
    rows
}

#[must_use]
pub fn forbidden_secret_keys<R, V>(
    workspace_env: &BTreeMap<String, V>,
    roles: &BTreeMap<String, R>,
    scope: &SecretsScopeTag,
    role_env: impl Fn(&R) -> &BTreeMap<String, V>,
) -> Vec<String> {
    match scope {
        SecretsScopeTag::Workspace => workspace_env.keys().cloned().collect(),
        SecretsScopeTag::Role(role) => roles
            .get(role)
            .map(|role_override| role_env(role_override).keys().cloned().collect())
            .unwrap_or_default(),
    }
}

pub fn set_secret_value<R, V>(
    workspace_env: &mut BTreeMap<String, V>,
    roles: &mut BTreeMap<String, R>,
    expanded_roles: &mut BTreeSet<String>,
    scope: &SecretsScopeTag,
    key: &str,
    value: V,
    mut ensure_role: impl FnMut(&mut BTreeMap<String, R>, &str),
    mut role_env_mut: impl FnMut(&mut R) -> &mut BTreeMap<String, V>,
) {
    match scope {
        SecretsScopeTag::Workspace => {
            workspace_env.insert(key.to_string(), value);
        }
        SecretsScopeTag::Role(role) => {
            ensure_role(roles, role);
            if let Some(role_override) = roles.get_mut(role) {
                role_env_mut(role_override).insert(key.to_string(), value);
                expanded_roles.insert(role.clone());
            }
        }
    }
}

#[must_use]
pub fn auth_flat_rows<K, R>(
    selected_kind: Option<K>,
    root_kinds: impl IntoIterator<Item = K>,
    roles: &BTreeMap<String, R>,
    allowed_role_count: usize,
    expanded_roles: &BTreeSet<String>,
    role_override_present: impl Fn(&K, &R) -> bool,
    effective_mode_needs_credential: impl Fn(&K, &str) -> bool,
) -> Vec<AuthRow<K>>
where
    K: Clone,
{
    let Some(kind) = selected_kind else {
        return root_kinds
            .into_iter()
            .map(|kind| AuthRow::AuthKindRow { kind })
            .collect();
    };

    let override_roles: Vec<String> = roles
        .iter()
        .filter(|(_, role)| role_override_present(&kind, role))
        .map(|(name, _)| name.clone())
        .collect();

    let mut rows = vec![AuthRow::WorkspaceMode { kind: kind.clone() }];
    if effective_mode_needs_credential(&kind, "") {
        rows.push(AuthRow::WorkspaceSource { kind: kind.clone() });
    }
    rows.push(AuthRow::Spacer);
    for role in &override_roles {
        let expanded = expanded_roles.contains(role);
        rows.push(AuthRow::RoleHeader {
            role: role.clone(),
            expanded,
        });
        if expanded {
            rows.push(AuthRow::RoleMode {
                role: role.clone(),
                kind: kind.clone(),
            });
            if effective_mode_needs_credential(&kind, role) {
                rows.push(AuthRow::RoleSource {
                    role: role.clone(),
                    kind: kind.clone(),
                });
            }
        }
    }
    let eligible_remaining = allowed_role_count.saturating_sub(override_roles.len());
    if !override_roles.is_empty() {
        rows.push(AuthRow::Spacer);
    }
    rows.push(AuthRow::AddSentinel {
        eligible: eligible_remaining,
    });
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_tab_move_plan_resets_local_view_state() {
        assert_eq!(
            editor_tab_move_plan(EditorTab::Secrets, 1, true),
            EditorTabMovePlan {
                active_tab: EditorTab::Auth,
                tab_bar_focused: true,
                active_row: 0,
                tab_scroll_x: 0,
                tab_scroll_y: 0,
                clear_auth_kind: false,
                clear_secret_view_state: true,
            }
        );
    }

    #[test]
    fn editor_tab_move_plan_clears_auth_kind_when_leaving_auth() {
        assert_eq!(
            editor_tab_move_plan(EditorTab::Auth, 1, false),
            EditorTabMovePlan {
                active_tab: EditorTab::General,
                tab_bar_focused: false,
                active_row: 0,
                tab_scroll_x: 0,
                tab_scroll_y: 0,
                clear_auth_kind: true,
                clear_secret_view_state: false,
            }
        );
    }

    #[test]
    fn editor_tab_select_plan_focuses_tab_and_clears_departed_state() {
        assert_eq!(
            editor_tab_select_plan(EditorTab::Secrets, EditorTab::Mounts),
            EditorTabSelectPlan {
                active_tab: EditorTab::Mounts,
                tab_bar_focused: true,
                active_row: 0,
                workspace_mounts_scroll_focused: false,
                clear_auth_kind: true,
                clear_secret_view_state: true,
            }
        );
    }

    #[test]
    fn editor_auth_kind_entry_plan_selects_kind_and_resets_view_state() {
        assert_eq!(
            enter_editor_auth_kind_plan(TestAuthKind::Claude),
            EditorAuthKindPlan {
                selected_kind: Some(TestAuthKind::Claude),
                active_row: 0,
                tab_scroll_x: 0,
                tab_scroll_y: 0,
            }
        );
    }

    #[test]
    fn editor_auth_kind_clear_plan_clears_kind_and_resets_view_state() {
        assert_eq!(
            clear_editor_auth_kind_plan::<TestAuthKind>(),
            EditorAuthKindPlan {
                selected_kind: None,
                active_row: 0,
                tab_scroll_x: 0,
                tab_scroll_y: 0,
            }
        );
    }

    #[test]
    fn editor_field_selection_plan_skips_rows_and_updates_scroll() {
        let plan = editor_field_selection_plan(1, 1, 4, &[2], 0, 8, 0);
        assert_eq!(plan.active_row, 3);
        assert!(plan.tab_scroll_y > 0);
    }

    #[derive(Default)]
    struct RoleEnv {
        env: BTreeMap<String, &'static str>,
    }

    #[test]
    fn secrets_flat_rows_include_expanded_role_keys() {
        let workspace_env = BTreeMap::from([("GLOBAL".to_string(), "x")]);
        let roles = BTreeMap::from([(
            "alpha".to_string(),
            RoleEnv {
                env: BTreeMap::from([("ROLE_KEY".to_string(), "x")]),
            },
        )]);
        let rows = secrets_flat_rows(
            &workspace_env,
            &roles,
            &BTreeSet::from(["alpha".to_string()]),
            |role| &role.env,
        );

        assert!(matches!(rows[0], SecretsRow::WorkspaceKeyRow(_)));
        assert!(rows.iter().any(
            |row| matches!(row, SecretsRow::RoleHeader { role, expanded: true } if role == "alpha")
        ));
        assert!(
            rows.iter()
                .any(|row| matches!(row, SecretsRow::RoleKeyRow { role, key } if role == "alpha" && key == "ROLE_KEY"))
        );
        assert!(
            rows.iter()
                .any(|row| matches!(row, SecretsRow::RoleAddSentinel(role) if role == "alpha"))
        );
    }

    #[test]
    fn secrets_flat_rows_collapse_role_keys() {
        let workspace_env = BTreeMap::new();
        let roles = BTreeMap::from([(
            "alpha".to_string(),
            RoleEnv {
                env: BTreeMap::from([("ROLE_KEY".to_string(), "x")]),
            },
        )]);
        let rows = secrets_flat_rows(&workspace_env, &roles, &BTreeSet::new(), |role| &role.env);

        assert!(matches!(rows[0], SecretsRow::WorkspaceAddSentinel));
        assert!(rows.iter().any(
            |row| matches!(row, SecretsRow::RoleHeader { role, expanded: false } if role == "alpha")
        ));
        assert!(!rows.iter().any(
            |row| matches!(row, SecretsRow::RoleKeyRow { role, key } if role == "alpha" && key == "ROLE_KEY")
        ));
    }

    #[test]
    fn forbidden_secret_keys_follow_scope() {
        let workspace_env = BTreeMap::from([("GLOBAL".to_string(), "x")]);
        let roles = BTreeMap::from([(
            "alpha".to_string(),
            RoleEnv {
                env: BTreeMap::from([("ROLE_KEY".to_string(), "x")]),
            },
        )]);

        assert_eq!(
            forbidden_secret_keys(&workspace_env, &roles, &SecretsScopeTag::Workspace, |role| {
                &role.env
            }),
            vec!["GLOBAL".to_string()]
        );
        assert_eq!(
            forbidden_secret_keys(
                &workspace_env,
                &roles,
                &SecretsScopeTag::Role("alpha".into()),
                |role| &role.env
            ),
            vec!["ROLE_KEY".to_string()]
        );
    }

    #[test]
    fn set_secret_value_creates_and_expands_role_scope() {
        let mut workspace_env = BTreeMap::new();
        let mut roles = BTreeMap::<String, RoleEnv>::new();
        let mut expanded = BTreeSet::new();

        set_secret_value(
            &mut workspace_env,
            &mut roles,
            &mut expanded,
            &SecretsScopeTag::Role("alpha".into()),
            "TOKEN",
            "secret",
            |roles, role| {
                roles.entry(role.to_string()).or_default();
            },
            |role| &mut role.env,
        );

        assert_eq!(roles["alpha"].env.get("TOKEN"), Some(&"secret"));
        assert!(expanded.contains("alpha"));
    }

    #[test]
    fn secret_row_targets_follow_scope() {
        let workspace = SecretsRow::WorkspaceKeyRow("TOKEN".to_string());
        let role = SecretsRow::RoleAddSentinel("alpha".to_string());

        assert_eq!(
            secret_delete_target_for_row(Some(&workspace)),
            Some((SecretsScopeTag::Workspace, "TOKEN".to_string()))
        );
        assert_eq!(
            secret_add_target_for_row(Some(&role)),
            Some((
                SecretsScopeTag::Role("alpha".to_string()),
                "New alpha environment key".to_string()
            ))
        );
        assert_eq!(
            secret_picker_target_for_row(Some(&role)),
            Some((SecretsScopeTag::Role("alpha".to_string()), None))
        );
        assert_eq!(
            secret_unmask_target_for_row(Some(&workspace), |_, _| true),
            Some((SecretsScopeTag::Workspace, "TOKEN".to_string()))
        );
        assert_eq!(
            secret_unmask_target_for_row(Some(&workspace), |_, _| false),
            None
        );
    }

    #[test]
    fn secret_enter_plan_handles_values_and_headers() {
        let key = SecretsRow::RoleKeyRow {
            role: "alpha".to_string(),
            key: "TOKEN".to_string(),
        };
        let collapsed = SecretsRow::RoleHeader {
            role: "alpha".to_string(),
            expanded: false,
        };
        let expanded = SecretsRow::RoleHeader {
            role: "alpha".to_string(),
            expanded: true,
        };

        assert_eq!(
            secret_enter_plan_for_row(Some(&key), |_, _| true),
            SecretsEnterPlan::EditValue {
                scope: SecretsScopeTag::Role("alpha".to_string()),
                key: "TOKEN".to_string()
            }
        );
        assert_eq!(
            secret_enter_plan_for_row(Some(&key), |_, _| false),
            SecretsEnterPlan::Noop
        );
        assert_eq!(
            secret_enter_plan_for_row(Some(&collapsed), |_, _| true),
            SecretsEnterPlan::ExpandRole("alpha".to_string())
        );
        assert_eq!(
            secret_enter_plan_for_row(Some(&expanded), |_, _| true),
            SecretsEnterPlan::Noop
        );
    }

    #[test]
    fn toggle_allowed_role_demotes_all_and_clears_default() {
        let role_names = vec!["alpha".to_string(), "beta".to_string()];
        let mut allowed_roles = Vec::new();
        let mut default_role = Some("alpha".to_string());

        toggle_allowed_role_at(&mut allowed_roles, &mut default_role, &role_names, 0);

        assert_eq!(allowed_roles, vec!["beta".to_string()]);
        assert_eq!(default_role, None);
    }

    #[test]
    fn toggle_allowed_role_collapses_full_roster_to_all() {
        let role_names = vec!["alpha".to_string(), "beta".to_string()];
        let mut allowed_roles = vec!["alpha".to_string()];
        let mut default_role = None;

        toggle_allowed_role_at(&mut allowed_roles, &mut default_role, &role_names, 1);

        assert!(allowed_roles.is_empty());
    }

    #[test]
    fn toggle_default_role_requires_effective_allowance() {
        let role_names = vec!["alpha".to_string(), "beta".to_string()];
        let mut default_role = None;

        toggle_default_role_at(&["alpha".to_string()], &mut default_role, &role_names, 1);
        assert_eq!(default_role, None);

        toggle_default_role_at(&["alpha".to_string()], &mut default_role, &role_names, 0);
        assert_eq!(default_role.as_deref(), Some("alpha"));

        toggle_default_role_at(&["alpha".to_string()], &mut default_role, &role_names, 0);
        assert_eq!(default_role, None);
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum TestAuthKind {
        Claude,
        Github,
    }

    struct RoleAuth {
        override_present: bool,
        needs_source: bool,
    }

    #[test]
    fn auth_flat_rows_root_view_lists_kinds() {
        let rows = auth_flat_rows::<TestAuthKind, RoleAuth>(
            None,
            [TestAuthKind::Claude, TestAuthKind::Github],
            &BTreeMap::new(),
            0,
            &BTreeSet::new(),
            |_, _| false,
            |_, _| false,
        );
        assert_eq!(
            rows,
            vec![
                AuthRow::AuthKindRow {
                    kind: TestAuthKind::Claude
                },
                AuthRow::AuthKindRow {
                    kind: TestAuthKind::Github
                },
            ]
        );
    }

    #[test]
    fn auth_flat_rows_detail_view_expands_role_source_rows() {
        let roles = BTreeMap::from([(
            "alpha".to_string(),
            RoleAuth {
                override_present: true,
                needs_source: true,
            },
        )]);
        let rows = auth_flat_rows(
            Some(TestAuthKind::Claude),
            [TestAuthKind::Claude, TestAuthKind::Github],
            &roles,
            3,
            &BTreeSet::from(["alpha".to_string()]),
            |_, role| role.override_present,
            |_, role| role.is_empty() || roles[role].needs_source,
        );
        assert_eq!(
            rows,
            vec![
                AuthRow::WorkspaceMode {
                    kind: TestAuthKind::Claude
                },
                AuthRow::WorkspaceSource {
                    kind: TestAuthKind::Claude
                },
                AuthRow::Spacer,
                AuthRow::RoleHeader {
                    role: "alpha".to_string(),
                    expanded: true,
                },
                AuthRow::RoleMode {
                    role: "alpha".to_string(),
                    kind: TestAuthKind::Claude
                },
                AuthRow::RoleSource {
                    role: "alpha".to_string(),
                    kind: TestAuthKind::Claude
                },
                AuthRow::Spacer,
                AuthRow::AddSentinel { eligible: 2 },
            ]
        );
    }
}
