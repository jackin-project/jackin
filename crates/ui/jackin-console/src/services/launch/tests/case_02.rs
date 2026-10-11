// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn launch_accounts_require_workspace_assignment_and_agent_support() {
    use jackin_config::{AccountConfig, AccountCredential, AiProvider};
    use jackin_core::{Agent, EnvValue};
    let mut config = AppConfig::default();
    for id in ["personal", "work"] {
        config.accounts.insert(
            id.into(),
            AccountConfig {
                enabled: true,
                name: id.into(),
                provider: AiProvider::Anthropic,
                credential: AccountCredential::Profile {
                    agent: Agent::Claude,
                    directory: format!("/profiles/{id}").into(),
                    xdg_roots: None,
                    source_selector: None,
                },
            },
        );
    }
    let mut workspace = WorkspaceConfig::default();
    workspace.accounts.push("work".into());
    config.workspaces.insert("demo".into(), workspace);
    config
        .env
        .insert("ZAI_API_KEY".into(), EnvValue::Plain("unregistered".into()));
    let choices = accounts_for_launch(&config, Some(&wn("demo")), Agent::Claude);
    assert_eq!(choices.len(), 1);
    assert_eq!(choices[0].id, "work");
    assert!(accounts_for_launch(&config, Some(&wn("demo")), Agent::Codex).is_empty());
    assert!(accounts_for_launch(&config, Some(&wn("missing")), Agent::Claude).is_empty());
    assert_eq!(accounts_for_launch(&config, None, Agent::Claude).len(), 2);
    config.accounts.get_mut("work").unwrap().enabled = false;
    assert!(accounts_for_launch(&config, Some(&wn("demo")), Agent::Claude).is_empty());
    assert!(account_choices(&config, Some(&wn("demo"))).is_empty());
}

#[test]
fn configured_picker_selection_preserves_exact_configuration() {
    let row = AccountChoice {
        id: "shared-account".into(),
        name: "Shared account".into(),
        provider: AiProvider::Anthropic,
        agents: vec![Agent::Claude],
        configuration_id: Some("claude-deep".into()),
        instance_id: None,
    };
    assert_eq!(
        row.into_launch_selection(),
        jackin_core::LaunchSelection::Configuration("claude-deep".into())
    );
}

#[test]
fn unconfigured_picker_selection_preserves_account() {
    let row = AccountChoice {
        id: "shared-account".into(),
        name: "Shared account".into(),
        provider: AiProvider::Anthropic,
        agents: vec![Agent::Claude],
        configuration_id: None,
        instance_id: None,
    };
    assert_eq!(
        row.into_launch_selection(),
        jackin_core::LaunchSelection::Account("shared-account".into())
    );
}
