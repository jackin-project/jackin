// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn wn(name: &str) -> WorkspaceName {
    WorkspaceName::parse(name).unwrap()
}

pub(super) fn agent_source_stub() -> RoleSource {
    RoleSource {
        git: "https://example.invalid/org/repo.git".to_owned(),
        trusted: true,
        env: BTreeMap::new(),
    }
}

pub(super) fn launch_workspace(
    workdir: &std::path::Path,
    allowed_roles: Vec<&str>,
) -> WorkspaceConfig {
    WorkspaceConfig {
        version: CURRENT_WORKSPACE_VERSION.to_owned(),
        workdir: workdir.display().to_string(),
        mounts: vec![MountConfig {
            src: workdir.display().to_string(),
            dst: workdir.display().to_string(),
            readonly: false,
            isolation: MountIsolation::Shared,
        }],
        allowed_roles: allowed_roles.into_iter().map(str::to_owned).collect(),
        default_role: None,
        default_agent: None,
        last_role: None,
        env: BTreeMap::new(),
        roles: BTreeMap::new(),
        keep_awake: KeepAwakeConfig::default(),
        accounts: Vec::new(),
        account_bindings: BTreeMap::new(),
        github: None,
        git_pull_on_entry: false,
        runtime: jackin_config::WorkspaceRuntimeConfig::default(),
        dirty_exit_policy: None,
        docker: None,
        default_launch: None,
    }
}

pub(super) fn api_key_account(name: &str, provider: AiProvider) -> AccountConfig {
    AccountConfig {
        enabled: true,
        name: name.into(),
        provider,
        credential: AccountCredential::ApiKey {
            value: "test-key".into(),
            base_url: None,
            model: None,
        },
    }
}

pub(super) fn agent_configuration(agent: Agent, account: &str) -> AgentConfiguration {
    AgentConfiguration {
        agent,
        account: account.into(),
        model: None,
        base_url: None,
        display_label: None,
        invoked_via_wrapper: None,
    }
}

pub(super) fn admission_config(project_dir: &std::path::Path) -> AppConfig {
    let mut config = AppConfig::default();
    config.roles.insert("smith".to_owned(), agent_source_stub());
    for (id, display) in [("a-claude", "A"), ("z-claude", "Z")] {
        config
            .accounts
            .insert(id.into(), api_key_account(display, AiProvider::Anthropic));
    }
    for (id, account) in [("claude-a", "a-claude"), ("claude-z", "z-claude")] {
        config
            .agent_configurations
            .insert(id.into(), agent_configuration(Agent::Claude, account));
    }
    let mut saved = launch_workspace(project_dir, vec!["smith"]);
    saved.accounts = vec!["a-claude".into(), "z-claude".into()];
    config.workspaces.insert("demo".to_owned(), saved);
    config
}
