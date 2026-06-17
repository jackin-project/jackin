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

/// Roles already carrying an override stay eligible: operators may add more
/// keys to an existing override.
#[must_use]
#[allow(unfulfilled_lint_expectations)]
#[expect(
    single_use_lifetimes,
    reason = "impl Iterator over borrowed String keys cannot use anonymous lifetimes on stable Rust"
)]
pub fn eligible_role_keys_for_override<'a>(
    registered_roles: impl Iterator<Item = &'a String>,
    workspace: &impl WorkspaceRoleAccess,
) -> Vec<String> {
    if workspace.allowed_roles().is_empty() {
        registered_roles.cloned().collect()
    } else {
        workspace.allowed_roles().to_vec()
    }
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
