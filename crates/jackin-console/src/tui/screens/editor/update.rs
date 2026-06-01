use std::collections::{BTreeMap, BTreeSet};

use super::model::{AuthRow, EditorTab, SecretsRow, SecretsScopeTag};

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
