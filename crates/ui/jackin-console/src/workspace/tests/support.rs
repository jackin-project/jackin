// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
struct TestWorkspace {
    allowed_roles: Vec<String>,
}

impl WorkspaceRoleAccess for TestWorkspace {
    fn allowed_roles(&self) -> &[String] {
        &self.allowed_roles
    }
}

fn ws_with_allowed(allowed: Vec<String>) -> TestWorkspace {
    TestWorkspace {
        allowed_roles: allowed,
    }
}

fn role(key: &str) -> RoleSelector {
    RoleSelector::parse(key).unwrap()
}

fn role_keys(roles: &[RoleSelector]) -> Vec<String> {
    roles.iter().map(RoleSelector::key).collect()
}

fn ws_with_role_overrides(allowed: &[&str], override_agents: &[&str]) -> WorkspaceConfig {
    let mut roles = std::collections::BTreeMap::new();
    for a in override_agents {
        roles.insert((*a).into(), WorkspaceRoleOverride::default());
    }
    WorkspaceConfig {
        allowed_roles: allowed.iter().map(|s| (*s).into()).collect(),
        roles,
        ..WorkspaceConfig::default()
    }
}

