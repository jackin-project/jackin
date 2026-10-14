// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::RoleSelector;

pub trait WorkspaceRoleAccess {
    fn allowed_roles(&self) -> &[String];
}

#[must_use]
pub fn allows_all_agents(ws: &impl WorkspaceRoleAccess) -> bool {
    ws.allowed_roles().is_empty()
}

#[must_use]
pub fn agent_is_effectively_allowed(ws: &impl WorkspaceRoleAccess, role: &str) -> bool {
    ws.allowed_roles().is_empty() || ws.allowed_roles().iter().any(|a| a == role)
}

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

#[must_use]
pub fn configured_roles(
    registered_roles: impl Iterator<Item = impl AsRef<str>>,
) -> Vec<RoleSelector> {
    registered_roles
        .filter_map(|key| RoleSelector::parse(key.as_ref()).ok())
        .collect()
}

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

impl WorkspaceRoleAccess for jackin_config::WorkspaceConfig {
    fn allowed_roles(&self) -> &[String] {
        &self.allowed_roles
    }
}

#[cfg(test)]
mod tests;
