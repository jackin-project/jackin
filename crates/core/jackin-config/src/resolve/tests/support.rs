// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn launch_config() -> (AppConfig, WorkspaceName) {
    use crate::{AccountConfig, AccountCredential, AgentConfiguration, AiProvider};
    use jackin_core::EnvValue;
    let mut config = AppConfig::default();
    for (id, display) in [("a-claude", "A"), ("z-claude", "Z")] {
        config.accounts.insert(
            id.to_owned(),
            AccountConfig {
                enabled: true,
                name: display.into(),
                provider: AiProvider::Anthropic,
                credential: AccountCredential::ApiKey {
                    value: EnvValue::Plain("test-key".into()),
                    base_url: None,
                    model: None,
                },
            },
        );
    }
    for (id, account) in [("claude-a", "a-claude"), ("claude-z", "z-claude")] {
        config.agent_configurations.insert(
            id.to_owned(),
            AgentConfiguration {
                agent: Agent::Claude,
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
        WorkspaceConfig {
            workdir: "/demo".into(),
            accounts: vec!["a-claude".into(), "z-claude".into()],
            ..Default::default()
        },
    );
    (config, ws)
}

pub(super) fn effective_ids(
    config: &AppConfig,
    ws: Option<&WorkspaceName>,
    role: &str,
) -> Option<Vec<String>> {
    config
        .effective_default_launch(ws, role)
        .map(<[String]>::to_vec)
}
