// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Workspace role-access rules shared by console surfaces.

use jackin_core::RoleSelector;

pub trait WorkspaceRoleAccess {
    fn allowed_roles(&self) -> &[String];
}

/// True when workspace uses "empty allowed roles = every agent allowed".
#[must_use]
pub fn allows_all_agents(ws: &impl WorkspaceRoleAccess) -> bool {
    ws.allowed_roles().is_empty()
}

/// True when `role` is effectively allowed, including empty-list shorthand.
#[must_use]
pub fn agent_is_effectively_allowed(ws: &impl WorkspaceRoleAccess, role: &str) -> bool {
    ws.allowed_roles().is_empty() || ws.allowed_roles().iter().any(|a| a == role)
}

/// Roles already carrying an override stay eligible: operators may add more
/// keys to an existing override.
#[must_use]
pub fn eligible_role_keys_for_override(
    registered_roles: impl Iterator<Item = impl AsRef<str>>,
    workspace: &impl WorkspaceRoleAccess,
) -> Vec<String> {
    if workspace.allowed_roles().is_empty() {
        registered_roles
            .map(|role| role.as_ref().to_owned())
            .collect()
    } else {
        workspace.allowed_roles().to_vec()
    }
}

/// Return configured roles permitted by a workspace's `allowed_roles`.
///
/// Empty `allowed_roles` means every configured role. Stale entries are
/// ignored, because this returns only roles present in `registered_roles`.
#[must_use]
pub fn eligible_roles_for_workspace(
    registered_roles: impl Iterator<Item = impl AsRef<str>>,
    workspace: &impl WorkspaceRoleAccess,
) -> Vec<RoleSelector> {
    configured_roles(registered_roles)
        .into_iter()
        .filter(|role| agent_is_effectively_allowed(workspace, &role.key()))
        .collect()
}

/// Return configured roles that parse as valid role selectors.
#[must_use]
pub fn configured_roles(
    registered_roles: impl Iterator<Item = impl AsRef<str>>,
) -> Vec<RoleSelector> {
    registered_roles
        .filter_map(|key| RoleSelector::parse(key.as_ref()).ok())
        .collect()
}

/// Return the index of the preferred role within `eligible`.
///
/// Priority is most-recent role first, then explicit default role. Returns
/// `None` when neither stored role exists in `eligible`.
#[must_use]
pub fn preferred_role_index(
    eligible: &[RoleSelector],
    last_role: Option<&str>,
    default_role: Option<&str>,
) -> Option<usize> {
    last_role
        .and_then(|last| eligible.iter().position(|role| role.key() == last))
        .or_else(|| {
            default_role.and_then(|default| eligible.iter().position(|role| role.key() == default))
        })
}

/// `WorkspaceRoleAccess` impl for `jackin_config::WorkspaceConfig`.
/// Lives here (trait definition site) to satisfy the orphan rule.
impl WorkspaceRoleAccess for jackin_config::WorkspaceConfig {
    fn allowed_roles(&self) -> &[String] {
        &self.allowed_roles
    }
}

#[cfg(test)]
mod tests;
