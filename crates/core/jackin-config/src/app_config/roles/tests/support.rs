// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn wn(name: &str) -> WorkspaceName {
    WorkspaceName::parse(name).unwrap()
}

pub(super) fn ws_with_github_env(env: BTreeMap<String, EnvValue>) -> WorkspaceConfig {
    WorkspaceConfig {
        github: Some(GithubAuthConfig {
            auth_forward: GithubAuthMode::Sync,
            env,
        }),
        ..WorkspaceConfig::default()
    }
}
