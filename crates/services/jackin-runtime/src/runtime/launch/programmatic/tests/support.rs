// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) const ROLE: &str = "donbeave/the-architect";

pub(super) fn selector() -> RoleSelector {
    RoleSelector::parse(ROLE).expect("role selector must parse")
}

pub(super) fn trusted_config() -> AppConfig {
    let mut config = AppConfig::default();
    config.roles.insert(
        selector().key(),
        RoleSource {
            git: "https://github.com/donbeave/the-architect".to_owned(),
            trusted: true,
            ..RoleSource::default()
        },
    );
    config
}

pub(super) fn untrusted_config() -> AppConfig {
    let mut config = trusted_config();
    if let Some(source) = config.roles.get_mut(&selector().key()) {
        source.trusted = false;
    }
    config
}

pub(super) fn opts() -> LoadOptions {
    LoadOptions::programmatic(Agent::Claude)
}

pub(super) fn codex_profile_account(name: &str) -> jackin_config::AccountConfig {
    jackin_config::AccountConfig {
        enabled: true,
        name: name.to_owned(),
        provider: jackin_config::AiProvider::OpenAi,
        credential: jackin_config::AccountCredential::Profile {
            agent: Agent::Codex,
            directory: format!("/profiles/{name}").into(),
            xdg_roots: None,
            source_selector: None,
        },
    }
}

pub(super) fn two_account_config() -> (AppConfig, jackin_core::WorkspaceName) {
    use jackin_config::WorkspaceConfig;
    let mut config = trusted_config();
    for (id, name) in [("private", "Private"), ("shared", "Shared")] {
        config
            .accounts
            .insert(id.to_owned(), codex_profile_account(name));
    }
    for (id, account) in [("codex-main", "private"), ("codex-alt", "shared")] {
        config.agent_configurations.insert(
            id.to_owned(),
            AgentConfiguration {
                agent: Agent::Codex,
                account: account.to_owned(),
                model: None,
                base_url: None,
                display_label: None,
                invoked_via_wrapper: None,
            },
        );
    }
    let workspace = WorkspaceConfig {
        accounts: vec!["private".to_owned(), "shared".to_owned()],
        ..WorkspaceConfig::default()
    };
    config.workspaces.insert("work".to_owned(), workspace);
    let workspace = jackin_core::WorkspaceName::parse("work").unwrap();
    (config, workspace)
}
