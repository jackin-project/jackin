// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn dry_run_workspace() -> crate::workspace::ResolvedWorkspace {
    crate::workspace::ResolvedWorkspace {
        name: "big-monorepo".to_owned(),
        label: "big-monorepo".to_owned(),
        workdir: "/workspace/big-monorepo".to_owned(),
        mounts: vec![MountConfig {
            src: "/host/big-monorepo".to_owned(),
            dst: "/workspace/big-monorepo".to_owned(),
            readonly: false,
            isolation: MountIsolation::Shared,
        }],
        keep_awake_enabled: false,
        default_agent: Some(Agent::Claude),
        git_pull_on_entry: false,
        mount_heal: jackin_config::MountHealReport::default(),
    }
}

pub(super) fn dry_run_image_plan() -> jackin_runtime::runtime::LaunchImagePlan {
    jackin_runtime::runtime::LaunchImagePlan {
        decision: "build_from_published",
        reason: Some("role_git_sha_changed"),
        image: "jk_the-architect:deadbee".to_owned(),
        base_image: Some("projectjackin/the-architect:latest".to_owned()),
        role_git_sha: Some("deadbee".to_owned()),
        published_image: Some("projectjackin/the-architect:latest".to_owned()),
        role_models: std::collections::BTreeMap::new(),
    }
}

pub(super) fn dry_run_identity_config() -> (AppConfig, jackin_core::WorkspaceName) {
    use jackin_config::{AccountConfig, AccountCredential, AgentConfiguration, AiProvider};
    let mut config = AppConfig::default();
    for (id, display, provider) in [
        ("a-claude", "Work", AiProvider::Anthropic),
        ("b-claude", "Personal", AiProvider::Anthropic),
        ("c-codex", "Work", AiProvider::OpenAi),
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
        ("claude-work", Agent::Claude, "a-claude"),
        ("claude-personal", Agent::Claude, "b-claude"),
        ("codex-work", Agent::Codex, "c-codex"),
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
    let ws = jackin_core::WorkspaceName::parse("demo").unwrap();
    config.workspaces.insert(
        ws.as_str().to_owned(),
        WorkspaceConfig {
            workdir: "/demo".into(),
            accounts: vec!["a-claude".into(), "b-claude".into(), "c-codex".into()],
            default_launch: Some(vec![
                "claude-work".into(),
                "claude-personal".into(),
                "codex-work".into(),
            ]),
            ..Default::default()
        },
    );
    (config, ws)
}
