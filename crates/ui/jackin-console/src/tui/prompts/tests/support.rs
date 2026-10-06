// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) const ROLE: &str = "the-architect";

pub(super) fn api_key_account(name: &str, provider: AiProvider) -> AccountConfig {
    AccountConfig {
        enabled: true,
        name: name.into(),
        provider,
        credential: AccountCredential::ApiKey {
            value: EnvValue::Plain("test-key".into()),
            base_url: None,
            model: None,
        },
    }
}

pub(super) fn test_config() -> (AppConfig, WorkspaceName) {
    let mut config = AppConfig::default();
    config.accounts.insert(
        "a-claude".into(),
        api_key_account("A", AiProvider::Anthropic),
    );
    config.accounts.insert(
        "z-claude".into(),
        api_key_account("Z", AiProvider::Anthropic),
    );
    config
        .accounts
        .insert("o-codex".into(), api_key_account("O", AiProvider::OpenAi));
    config.accounts.insert(
        "outside".into(),
        api_key_account("Outside", AiProvider::Anthropic),
    );
    let ws = WorkspaceName::parse("demo").unwrap();
    config.workspaces.insert(
        ws.as_str().into(),
        WorkspaceConfig {
            workdir: "/demo".into(),
            accounts: vec!["a-claude".into(), "z-claude".into(), "o-codex".into()],
            ..Default::default()
        },
    );
    (config, ws)
}

pub(super) fn set_role_binding(
    config: &mut AppConfig,
    ws: &str,
    role: &str,
    agent: Agent,
    id: &str,
) {
    config.workspaces.get_mut(ws).unwrap().roles.insert(
        role.into(),
        WorkspaceRoleOverride {
            account_bindings: BTreeMap::from([(agent, id.to_owned())]),
            ..Default::default()
        },
    );
}

pub(super) fn update_role_binding(
    config: &mut AppConfig,
    ws: &str,
    role: &str,
    agent: Agent,
    id: &str,
) {
    config
        .workspaces
        .get_mut(ws)
        .unwrap()
        .roles
        .get_mut(role)
        .expect("role override must exist")
        .account_bindings
        .insert(agent, id.to_owned());
}

pub(super) fn eligible_ids(selection: &LaunchAccountSelection) -> Vec<&str> {
    let LaunchAccountSelection::Pick(accounts) = selection else {
        panic!("expected picker selection; got {selection:?}");
    };
    accounts.iter().map(|account| account.id.as_str()).collect()
}

pub(super) fn agent_configuration(agent: Agent, account: &str) -> AgentConfiguration {
    AgentConfiguration {
        agent,
        account: account.to_owned(),
        model: None,
        base_url: None,
        display_label: None,
        invoked_via_wrapper: None,
    }
}

pub(super) fn launch_config() -> (AppConfig, WorkspaceName) {
    let (mut config, ws) = test_config();
    for (id, agent, account) in [
        ("claude-a", Agent::Claude, "a-claude"),
        ("claude-z", Agent::Claude, "z-claude"),
        ("codex-o", Agent::Codex, "o-codex"),
        ("claude-out", Agent::Claude, "outside"),
    ] {
        config
            .agent_configurations
            .insert(id.into(), agent_configuration(agent, account));
    }
    (config, ws)
}

pub(super) fn set_workspace_default(config: &mut AppConfig, ws: &str, ids: &[&str]) {
    config.workspaces.get_mut(ws).unwrap().default_launch =
        Some(ids.iter().map(ToString::to_string).collect());
}

pub(super) fn set_role_default(config: &mut AppConfig, ws: &str, role: &str, ids: &[&str]) {
    config
        .workspaces
        .get_mut(ws)
        .unwrap()
        .roles
        .entry(role.into())
        .or_default()
        .default_launch = Some(ids.iter().map(ToString::to_string).collect());
}
