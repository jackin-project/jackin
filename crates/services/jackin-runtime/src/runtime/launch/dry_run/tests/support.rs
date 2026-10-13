// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) const ROLE: &str = "agent-smith";

pub(super) const WS: &str = "myapp";

pub(super) fn claude_profile_account(name: &str) -> AccountConfig {
    AccountConfig {
        enabled: true,
        name: name.to_owned(),
        provider: AiProvider::Anthropic,
        credential: AccountCredential::Profile {
            agent: Agent::Claude,
            directory: format!("/profiles/{name}").into(),
            xdg_roots: None,
            source_selector: None,
        },
    }
}

pub(super) fn configuration(agent: Agent, account: &str) -> AgentConfiguration {
    AgentConfiguration {
        agent,
        account: account.to_owned(),
        model: None,
        base_url: None,
        display_label: None,
        invoked_via_wrapper: None,
    }
}

pub(super) fn matrix_config() -> (AppConfig, WorkspaceName) {
    let mut config = AppConfig::default();
    for id in ["c-lab", "c-home", "c-work"] {
        config
            .accounts
            .insert(id.to_owned(), claude_profile_account(id));
    }
    for (id, account) in [
        ("cfg-r", "c-lab"),
        ("cfg-r2", "c-home"),
        ("cfg-w", "c-work"),
        ("cfg-g", "c-home"),
    ] {
        config
            .agent_configurations
            .insert(id.to_owned(), configuration(Agent::Claude, account));
    }
    config.workspaces.insert(
        WS.to_owned(),
        WorkspaceConfig {
            workdir: "/app".to_owned(),
            accounts: vec!["c-home".to_owned(), "c-lab".to_owned(), "c-work".to_owned()],
            ..WorkspaceConfig::default()
        },
    );
    let workspace = WorkspaceName::parse(WS).unwrap();
    (config, workspace)
}

pub(super) fn set_role_list(config: &mut AppConfig, ids: &[&str]) {
    config
        .workspaces
        .get_mut(WS)
        .unwrap()
        .roles
        .entry(ROLE.to_owned())
        .or_default()
        .default_launch = Some(ids.iter().map(ToString::to_string).collect());
}

pub(super) fn identity(
    config: &AppConfig,
    workspace: &WorkspaceName,
) -> anyhow::Result<DryRunIdentity> {
    resolve_dry_run_identity(config, Agent::Claude, Some(workspace), ROLE, false)
}

pub(super) fn admission(
    config: &AppConfig,
    workspace: &WorkspaceName,
) -> anyhow::Result<Vec<ResolvedInstance>> {
    Ok(jackin_config::resolve_launch(
        config,
        Some(workspace),
        ROLE,
        None,
        Some(Agent::Claude),
    )?)
}

pub(super) fn admitted_pairs(instances: &[ResolvedInstance]) -> Vec<(&str, &str)> {
    instances
        .iter()
        .map(|instance| (instance.config_id.as_str(), instance.account_id.as_str()))
        .collect()
}

pub(super) fn mixed_model_projection_config() -> (AppConfig, WorkspaceName) {
    let mut config = AppConfig::default();
    for (id, provider, model) in [
        ("openai", AiProvider::OpenAi, None),
        ("zai", AiProvider::Zai, Some("glm-account-default")),
    ] {
        config.accounts.insert(
            id.to_owned(),
            AccountConfig {
                enabled: true,
                name: id.to_owned(),
                provider,
                credential: AccountCredential::ApiKey {
                    value: jackin_core::EnvValue::Plain("fixture-key".to_owned()),
                    base_url: None,
                    model: model.map(ToOwned::to_owned),
                },
            },
        );
    }
    for (id, agent, account) in [
        ("codex-main", Agent::Codex, "openai"),
        ("opencode-openai", Agent::Opencode, "openai"),
        ("opencode-zai", Agent::Opencode, "zai"),
    ] {
        config.agent_configurations.insert(
            id.to_owned(),
            AgentConfiguration {
                agent,
                account: account.to_owned(),
                model: None,
                base_url: None,
                display_label: None,
                invoked_via_wrapper: None,
            },
        );
    }
    config.default_launch = Some(vec!["codex-main".to_owned(), "opencode-zai".to_owned()]);
    config.workspaces.insert(
        WS.to_owned(),
        WorkspaceConfig {
            workdir: "/workspace".to_owned(),
            accounts: vec!["openai".to_owned(), "zai".to_owned()],
            ..WorkspaceConfig::default()
        },
    );
    (config, WorkspaceName::parse(WS).unwrap())
}

pub(super) fn mixed_role_model_defaults() -> std::collections::BTreeMap<Agent, String> {
    std::collections::BTreeMap::from([
        (Agent::Codex, "codex-role-default".to_owned()),
        (Agent::Opencode, "opencode-role-default".to_owned()),
    ])
}
