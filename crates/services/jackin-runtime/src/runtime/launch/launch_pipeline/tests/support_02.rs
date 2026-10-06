// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn breadcrumb_config() -> (AppConfig, WorkspaceName) {
    use jackin_config::{AccountConfig, AccountCredential, AgentConfiguration, AiProvider};
    let mut config = AppConfig::default();
    for (id, display, provider) in [
        ("a-claude", "A", AiProvider::Anthropic),
        ("z-claude", "Z", AiProvider::Anthropic),
        ("c-codex", "C", AiProvider::OpenAi),
    ] {
        config.accounts.insert(
            id.to_owned(),
            AccountConfig {
                enabled: true,
                name: display.into(),
                provider,
                credential: AccountCredential::ApiKey {
                    value: jackin_core::EnvValue::Plain("test-key".into()),
                    base_url: None,
                    model: None,
                },
            },
        );
    }
    for (id, agent, account) in [
        ("claude-a", Agent::Claude, "a-claude"),
        ("claude-z", Agent::Claude, "z-claude"),
        ("codex-c", Agent::Codex, "c-codex"),
    ] {
        config.agent_configurations.insert(
            id.to_owned(),
            AgentConfiguration {
                agent,
                account: account.into(),
                model: None,
                base_url: None,
                display_label: None,
                invoked_via_wrapper: None,
            },
        );
    }
    let ws = WorkspaceName::parse("demo").unwrap();
    config.workspaces.insert(
        ws.as_str().to_owned(),
        jackin_config::WorkspaceConfig {
            workdir: "/demo".into(),
            accounts: vec!["a-claude".into(), "z-claude".into(), "c-codex".into()],
            default_launch: Some(vec!["claude-a".into(), "claude-z".into(), "codex-c".into()]),
            ..Default::default()
        },
    );
    (config, ws)
}
