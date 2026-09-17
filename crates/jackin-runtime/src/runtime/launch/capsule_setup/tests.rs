// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_config::{AccountConfig, AccountCredential, AiProvider, AppConfig, AuthForwardMode};
use jackin_core::Agent;

fn api_key_account(provider: AiProvider, model: Option<&str>) -> AccountConfig {
    AccountConfig {
        enabled: true,
        name: "Test".into(),
        provider,
        credential: AccountCredential::ApiKey {
            value: "fixture-key".into(),
            base_url: None,
            model: model.map(str::to_owned),
        },
    }
}

fn instance(config_id: &str, agent: Agent, account_id: &str) -> jackin_config::ResolvedInstance {
    jackin_config::ResolvedInstance {
        config_id: config_id.into(),
        agent,
        account_id: account_id.into(),
        model: None,
        base_url: None,
        label: config_id.into(),
        synthesized: true,
    }
}

fn manifest_with(temp: &tempfile::TempDir, agents: &[&str]) -> jackin_manifest::RoleManifest {
    let mut role = format!(
        "version = \"v1alpha5\"\ndockerfile = \"Dockerfile\"\nagents = [{}]\n",
        agents
            .iter()
            .map(|agent| format!("\"{agent}\""))
            .collect::<Vec<_>>()
            .join(", ")
    );
    for agent in agents {
        role.push_str(&format!("\n[{agent}]\n"));
        if *agent == "claude" {
            role.push_str("model = \"sonnet\"\n");
        }
    }
    std::fs::write(temp.path().join("jackin.role.toml"), role).unwrap();
    std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    jackin_manifest::load_role_manifest(temp.path()).unwrap()
}

#[test]
fn selections_are_keyed_by_config_id() {
    let mut config = AppConfig::default();
    config
        .accounts
        .insert("work".into(), api_key_account(AiProvider::Anthropic, None));
    config.accounts.insert(
        "personal".into(),
        AccountConfig {
            enabled: true,
            name: "Personal".into(),
            provider: AiProvider::OpenAi,
            credential: AccountCredential::Profile {
                agent: Agent::Codex,
                directory: "/accounts/personal".into(),
                xdg_roots: None,
            },
        },
    );
    let instances = vec![
        instance("claude-work", Agent::Claude, "work"),
        instance("codex-personal", Agent::Codex, "personal"),
    ];
    let selections = account_auth_selections(&config, &instances).unwrap();
    assert_eq!(selections.len(), 2);
    assert_eq!(selections["claude-work"], (AuthForwardMode::ApiKey, None));
    assert_eq!(
        selections["codex-personal"],
        (AuthForwardMode::Sync, Some("/accounts/personal".into()))
    );
}

#[test]
fn selections_reject_unknown_accounts() {
    let config = AppConfig::default();
    account_auth_selections(&config, &[instance("ghost@codex", Agent::Codex, "ghost")])
        .unwrap_err();
}

#[test]
fn auth_modes_are_keyed_by_config_id() {
    let mut config = AppConfig::default();
    config
        .accounts
        .insert("work".into(), api_key_account(AiProvider::Anthropic, None));
    config.accounts.insert(
        "personal".into(),
        AccountConfig {
            enabled: true,
            name: "Personal".into(),
            provider: AiProvider::Anthropic,
            credential: AccountCredential::OAuthToken {
                agent: Agent::Claude,
                value: "fixture-token".into(),
            },
        },
    );
    let modes = capsule_auth_modes(
        &config,
        &[
            instance("claude-work", Agent::Claude, "work"),
            instance("claude-personal", Agent::Claude, "personal"),
        ],
    )
    .unwrap();
    assert_eq!(modes.len(), 2);
    assert_eq!(modes["claude-work"], AuthForwardMode::ApiKey.to_string());
    assert_eq!(
        modes["claude-personal"],
        AuthForwardMode::OAuthToken.to_string()
    );
}

#[test]
fn account_models_use_effective_instance_model_per_instance() {
    let mut config = AppConfig::default();
    config.accounts.insert(
        "work".into(),
        api_key_account(AiProvider::OpenAi, Some("account-model")),
    );
    config.accounts.insert(
        "other".into(),
        api_key_account(AiProvider::OpenAi, Some("other-model")),
    );
    let mut first = instance("codex-work", Agent::Codex, "work");
    first.model = Some("config-override".into());
    let mut second = instance("codex-other", Agent::Codex, "other");
    second.model = Some("other-model".into());
    let mut launch = jackin_protocol::CapsuleConfig {
        models: std::collections::BTreeMap::from([("codex-work".into(), "native-model".into())]),
        ..Default::default()
    };
    apply_account_models(&mut launch, &config, &[first, second]).unwrap();
    assert_eq!(launch.models["codex-work"], "config-override");
    assert_eq!(launch.models["codex-other"], "other-model");
}

#[test]
fn account_models_prefix_opencode_and_skip_missing() {
    let mut config = AppConfig::default();
    config.accounts.insert(
        "work".into(),
        api_key_account(AiProvider::Zai, Some("glm-5.3")),
    );
    config
        .accounts
        .insert("plain".into(), api_key_account(AiProvider::OpenAi, None));
    let mut opencode = instance("opencode-work", Agent::Opencode, "work");
    opencode.model = Some("glm-5.3".into());
    let codex = instance("codex-plain", Agent::Codex, "plain");
    let mut launch = jackin_protocol::CapsuleConfig {
        models: std::collections::BTreeMap::from([("codex-plain".into(), "native-model".into())]),
        ..Default::default()
    };
    apply_account_models(&mut launch, &config, &[opencode, codex]).unwrap();
    assert_eq!(launch.models["opencode-work"], "zai-coding-plan/glm-5.3");
    assert_eq!(launch.models["codex-plain"], "native-model");
}

#[test]
fn capsule_config_fans_manifest_models_out_per_instance() {
    let temp = tempfile::tempdir().unwrap();
    let manifest = manifest_with(&temp, &["claude", "codex"]);
    let selector = jackin_core::RoleSelector::new(Some("chainargos"), "the-architect");
    let config = capsule_config(
        &selector,
        "/workspace",
        &manifest,
        "ask",
        Vec::new(),
        &[
            instance("claude-work", Agent::Claude, "work"),
            instance("claude-personal", Agent::Claude, "personal"),
        ],
    );
    assert_eq!(config.role, "chainargos/the-architect");
    assert_eq!(config.workdir, "/workspace");
    assert_eq!(config.instances, vec!["claude-work", "claude-personal"]);
    assert_eq!(
        config.agents,
        std::collections::BTreeMap::from([
            ("claude-work".into(), "claude".into()),
            ("claude-personal".into(), "claude".into()),
        ])
    );
    assert_eq!(config.agent_for_instance("claude-work"), Some("claude"));
    assert_eq!(config.agent_for_instance("unknown"), None);
    assert_eq!(
        config.models,
        std::collections::BTreeMap::from([
            ("claude-work".into(), "sonnet".into()),
            ("claude-personal".into(), "sonnet".into()),
        ])
    );
}

#[test]
fn capsule_config_carries_instance_accounts_and_labels() {
    let temp = tempfile::tempdir().unwrap();
    let manifest = manifest_with(&temp, &["claude"]);
    let selector = jackin_core::RoleSelector::new(Some("chainargos"), "the-architect");
    let mut work = instance("claude-work", Agent::Claude, "work");
    work.label = "Claude · Work".into();
    let mut personal = instance("claude-personal", Agent::Claude, "personal");
    personal.label = "Personal Claude".into();
    let config = capsule_config(
        &selector,
        "/workspace",
        &manifest,
        "ask",
        Vec::new(),
        &[work, personal],
    );
    assert_eq!(
        config.accounts,
        std::collections::BTreeMap::from([
            ("claude-work".into(), "work".into()),
            ("claude-personal".into(), "personal".into()),
        ])
    );
    assert_eq!(config.account_for_instance("claude-work"), Some("work"));
    assert_eq!(config.account_for_instance("unknown"), None);
    // Operator label overrides pass through verbatim; names only, never secrets.
    assert_eq!(
        config.labels,
        std::collections::BTreeMap::from([
            ("claude-work".into(), "Claude · Work".into()),
            ("claude-personal".into(), "Personal Claude".into()),
        ])
    );
    assert_eq!(
        config.label_for_instance("claude-personal"),
        Some("Personal Claude")
    );
    assert_eq!(config.label_for_instance("unknown"), None);
}
