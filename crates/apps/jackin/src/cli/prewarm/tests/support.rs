// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn config_with_workspace_default(agent: Option<Agent>) -> AppConfig {
    let mut config = AppConfig::default();
    config.roles.insert(
        "agent-smith".to_owned(),
        jackin_config::RoleSource {
            git: "https://example.invalid/agent-smith.git".to_owned(),
            trusted: true,
            env: std::collections::BTreeMap::new(),
        },
    );
    config.workspaces.insert(
        "jackin".to_owned(),
        jackin_config::WorkspaceConfig {
            workdir: "/workspace".to_owned(),
            default_role: Some("agent-smith".to_owned()),
            default_agent: agent,
            ..jackin_config::WorkspaceConfig::default()
        },
    );
    config
}

pub(super) fn prewarm_args(flags: PrewarmFlags) -> PrewarmArgs {
    PrewarmArgs {
        agents: Vec::new(),
        flags,
        role: None,
        workspace: None,
        role_git: None,
        role_branch: None,
    }
}
