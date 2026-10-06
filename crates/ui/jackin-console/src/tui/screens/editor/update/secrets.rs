// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Editor secret row plans.

use super::super::model::{SecretsEnterPlan, SecretsRow, SecretsScopeTag};
use std::collections::{BTreeMap, BTreeSet};

#[must_use]
pub fn secret_delete_target_for_row(row: Option<&SecretsRow>) -> Option<(SecretsScopeTag, String)> {
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
pub fn secret_add_target_for_row(row: Option<&SecretsRow>) -> Option<SecretsScopeTag> {
    match row? {
        SecretsRow::WorkspaceKeyRow(_) | SecretsRow::WorkspaceAddSentinel => {
            Some(SecretsScopeTag::Workspace)
        }
        SecretsRow::RoleHeader { role, .. }
        | SecretsRow::RoleKeyRow { role, .. }
        | SecretsRow::RoleAddSentinel(role) => Some(SecretsScopeTag::Role(role.clone())),
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

#[expect(
    clippy::too_many_arguments,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
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
            workspace_env.insert(key.to_owned(), value);
        }
        SecretsScopeTag::Role(role) => {
            ensure_role(roles, role);
            if let Some(role_override) = roles.get_mut(role) {
                role_env_mut(role_override).insert(key.to_owned(), value);
                expanded_roles.insert(role.clone());
            }
        }
    }
}
