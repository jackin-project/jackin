// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn profile(name: &str) -> AccountConfig {
    AccountConfig {
        enabled: true,
        name: name.into(),
        provider: AiProvider::Anthropic,
        credential: AccountCredential::Profile {
            agent: Agent::Claude,
            directory: PathBuf::from("/profiles").join(name),
            xdg_roots: None,
            source_selector: None,
        },
    }
}

pub(super) fn config() -> (AppConfig, WorkspaceName) {
    let mut cfg = AppConfig::default();
    cfg.accounts.insert("personal".into(), profile("Personal"));
    cfg.accounts.insert("work".into(), profile("Work"));
    let ws = WorkspaceName::parse("project").unwrap();
    cfg.workspaces
        .insert(ws.as_str().into(), WorkspaceConfig::default());
    (cfg, ws)
}

pub(super) fn api_key(provider: AiProvider, model: Option<&str>) -> AccountConfig {
    AccountConfig {
        enabled: true,
        name: format!("{provider} key"),
        provider,
        credential: AccountCredential::ApiKey {
            value: EnvValue::from("fixture-key"),
            base_url: None,
            model: model.map(str::to_owned),
        },
    }
}

pub(super) fn launch_fixture() -> (AppConfig, WorkspaceName) {
    let mut cfg = AppConfig::default();
    cfg.accounts.insert("claude-work".into(), profile("Work"));
    let mut personal = profile("Personal");
    personal.provider = AiProvider::Anthropic;
    cfg.accounts.insert("claude-personal".into(), personal);
    cfg.accounts.insert(
        "codex-work".into(),
        AccountConfig {
            enabled: true,
            name: "Work".into(),
            provider: AiProvider::OpenAi,
            credential: AccountCredential::Profile {
                agent: Agent::Codex,
                directory: PathBuf::from("/profiles/codex-work"),
                xdg_roots: None,
                source_selector: None,
            },
        },
    );
    cfg.accounts
        .insert("zai-key".into(), api_key(AiProvider::Zai, None));
    for (id, agent, account) in [
        ("claude-a", Agent::Claude, "claude-work"),
        ("claude-b", Agent::Claude, "claude-personal"),
        ("codex-c", Agent::Codex, "codex-work"),
    ] {
        cfg.agent_configurations.insert(
            id.into(),
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
    let ws = WorkspaceName::parse("project").unwrap();
    let workspace = WorkspaceConfig {
        accounts: vec![
            "claude-work".into(),
            "claude-personal".into(),
            "codex-work".into(),
        ],
        ..Default::default()
    };
    cfg.workspaces.insert(ws.as_str().into(), workspace);
    (cfg, ws)
}
