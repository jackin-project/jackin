//! Workspace role-access rules shared by console surfaces.

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

#[cfg(test)]
mod tests;
