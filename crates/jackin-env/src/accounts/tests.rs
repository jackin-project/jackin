// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_config::{AccountConfig, AccountCredential, AgentConfiguration, AiProvider};
use jackin_core::{Agent, EnvValue, Extended};

struct NoSecrets;
impl OpRunner for NoSecrets {
    fn read(&self, _: &str) -> anyhow::Result<String> {
        anyhow::bail!("unexpected secret lookup")
    }
}
fn account_with_model(value: EnvValue, model: Option<&str>) -> AccountConfig {
    AccountConfig {
        enabled: true,
        name: "Test".into(),
        provider: AiProvider::OpenAi,
        credential: AccountCredential::ApiKey {
            value,
            base_url: None,
            model: model.map(str::to_owned),
        },
    }
}

fn account(value: &str) -> AccountConfig {
    account_with_model(EnvValue::from(value), None)
}

fn configuration(agent: Agent, account: &str) -> AgentConfiguration {
    AgentConfiguration {
        agent,
        account: account.into(),
        model: None,
        base_url: None,
        display_label: None,
        invoked_via_wrapper: None,
    }
}

fn configuration_with_endpoint(
    agent: Agent,
    account: &str,
    model: Option<&str>,
    base_url: &str,
) -> AgentConfiguration {
    AgentConfiguration {
        agent,
        account: account.into(),
        model: model.map(str::to_owned),
        base_url: Some(base_url.into()),
        display_label: None,
        invoked_via_wrapper: None,
    }
}

fn launch(cfg: &AppConfig, ids: &[&str]) -> Vec<jackin_config::ResolvedInstance> {
    let ids: Vec<String> = ids.iter().map(ToString::to_string).collect();
    jackin_config::resolve_launch(cfg, None, "role", Some(&ids), None).unwrap()
}

#[test]
fn unknown_account_rejected() {
    let cfg = AppConfig::default();
    let instances = vec![jackin_config::ResolvedInstance {
        config_id: "ghost@codex".into(),
        agent: Agent::Codex,
        account_id: "ghost".into(),
        model: None,
        base_url: None,
        xdg_roots: None,
        label: "Ghost".into(),
        synthesized: true,
    }];
    resolve_instance_env_with(&cfg, &instances, None, "role", &NoSecrets, |_| {
        Err(std::env::VarError::NotPresent)
    })
    .unwrap_err();
}

#[test]
fn assigned_key_resolves_host_reference_without_reading_other_accounts() {
    let mut cfg = AppConfig::default();
    cfg.accounts.insert("work".into(), account("$WORK_TOKEN"));
    cfg.accounts
        .insert("unselected".into(), account("$ABSENT_TOKEN"));
    cfg.agent_configurations
        .insert("primary".into(), configuration(Agent::Codex, "work"));
    let instances = launch(&cfg, &["primary"]);
    let env = resolve_instance_env_with(&cfg, &instances, None, "codex", &NoSecrets, |name| {
        assert_eq!(name, "WORK_TOKEN");
        Ok("test-key".into())
    })
    .unwrap();
    assert_eq!(
        env.for_instance("primary").unwrap()["OPENAI_API_KEY"],
        "test-key"
    );
    assert_eq!(env.instance("primary").unwrap().agent, "codex");
    assert_eq!(env.instance("primary").unwrap().account_id, "work");
}

#[test]
fn different_accounts_resolve_into_separate_instance_environments() {
    let mut cfg = AppConfig::default();
    cfg.accounts.insert("codex".into(), account("test-one"));
    cfg.accounts.insert(
        "opencode".into(),
        account_with_model(EnvValue::from("test-two"), Some("gpt-4o")),
    );
    cfg.agent_configurations
        .insert("codex-main".into(), configuration(Agent::Codex, "codex"));
    cfg.agent_configurations.insert(
        "opencode-main".into(),
        configuration(Agent::Opencode, "opencode"),
    );
    let instances = launch(&cfg, &["codex-main", "opencode-main"]);
    let env = resolve_instance_env_with(&cfg, &instances, None, "role", &NoSecrets, |_| {
        Err(std::env::VarError::NotPresent)
    })
    .unwrap();
    assert_eq!(
        env.for_instance("codex-main").unwrap()["OPENAI_API_KEY"],
        "test-one"
    );
    assert_eq!(
        env.for_instance("opencode-main").unwrap()["OPENAI_API_KEY"],
        "test-two"
    );
}

#[test]
fn routed_codex_configuration_model_overrides_missing_account_model() {
    let mut cfg = AppConfig::default();
    cfg.accounts.insert(
        "work".into(),
        AccountConfig {
            enabled: true,
            name: "Work".into(),
            provider: AiProvider::Moonshot,
            credential: AccountCredential::ApiKey {
                value: EnvValue::from("work-secret"),
                base_url: Some("https://account.example/v1".into()),
                model: None,
            },
        },
    );
    cfg.agent_configurations.insert(
        "codex-work".into(),
        configuration_with_endpoint(
            Agent::Codex,
            "work",
            Some("k3-256k"),
            "https://route.example/v1",
        ),
    );

    let instances = launch(&cfg, &["codex-work"]);
    assert_eq!(instances[0].model.as_deref(), Some("k3-256k"));
    assert_eq!(
        instances[0].base_url.as_deref(),
        Some("https://route.example/v1")
    );
    let env = resolve_instance_env_with(&cfg, &instances, None, "role", &NoSecrets, |_| {
        Err(std::env::VarError::NotPresent)
    })
    .expect("configuration model must satisfy routed Codex credential validation");

    assert_eq!(
        env.for_instance("codex-work").unwrap(),
        &BTreeMap::from([
            ("KIMI_API_KEY".into(), "work-secret".into()),
            ("OPENAI_BASE_URL".into(), "https://route.example/v1".into(),),
        ])
    );
}

#[test]
fn routed_opencode_cli_model_can_be_applied_after_credential_resolution() {
    let mut cfg = AppConfig::default();
    cfg.accounts.insert(
        "work".into(),
        AccountConfig {
            enabled: true,
            name: "Work".into(),
            provider: AiProvider::Moonshot,
            credential: AccountCredential::ApiKey {
                value: EnvValue::from("work-secret"),
                base_url: Some("https://account.example/v1".into()),
                model: None,
            },
        },
    );
    cfg.agent_configurations.insert(
        "opencode-work".into(),
        configuration_with_endpoint(Agent::Opencode, "work", None, "https://route.example/v1"),
    );

    // The CLI model is not part of ResolvedInstance; it is fanned out later
    // into the private OpenCode configuration. Credential resolution must not
    // reject the launch before that model reaches its consumer.
    let instances = launch(&cfg, &["opencode-work"]);
    assert!(instances[0].model.is_none());
    let env = resolve_instance_env_with(&cfg, &instances, None, "role", &NoSecrets, |_| {
        Err(std::env::VarError::NotPresent)
    })
    .expect("credential resolution must defer the later OpenCode CLI model");

    let instance = env.for_instance("opencode-work").unwrap();
    assert_eq!(instance["MOONSHOT_API_KEY"], "work-secret");
    assert!(!instance.values().any(|value| value == DEFERRED_MODEL));
    assert!(
        !instance
            .values()
            .any(|value| value == "https://route.example/v1")
    );
}

#[test]
fn same_agent_instances_keep_only_their_own_vars() {
    let mut cfg = AppConfig::default();
    for (id, key) in [("work", "work-key"), ("personal", "personal-key")] {
        cfg.accounts.insert(
            id.into(),
            AccountConfig {
                enabled: true,
                name: id.into(),
                provider: AiProvider::Anthropic,
                credential: AccountCredential::ApiKey {
                    value: EnvValue::from(key),
                    base_url: None,
                    model: None,
                },
            },
        );
        cfg.agent_configurations
            .insert(format!("claude-{id}"), configuration(Agent::Claude, id));
    }
    let instances = launch(&cfg, &["claude-work", "claude-personal"]);
    let env = resolve_instance_env_with(&cfg, &instances, None, "role", &NoSecrets, |_| {
        Err(std::env::VarError::NotPresent)
    })
    .unwrap();
    let work = env.for_instance("claude-work").unwrap();
    let personal = env.for_instance("claude-personal").unwrap();
    assert_eq!(work["ANTHROPIC_API_KEY"], "work-key");
    assert_eq!(personal["ANTHROPIC_API_KEY"], "personal-key");
    assert!(!work.values().any(|v| v == "personal-key"));
    assert!(!personal.values().any(|v| v == "work-key"));
    assert_eq!(env.instance("claude-work").unwrap().account_id, "work");
    assert_eq!(
        env.instance("claude-personal").unwrap().account_id,
        "personal"
    );
}

#[test]
fn same_account_claude_and_kimi_instances_keep_distinct_endpoints() {
    let mut cfg = AppConfig::default();
    cfg.accounts.insert(
        "shared".into(),
        AccountConfig {
            enabled: true,
            name: "Shared Kimi".into(),
            provider: AiProvider::Moonshot,
            credential: AccountCredential::ApiKey {
                value: EnvValue::from("shared-secret"),
                base_url: Some("https://account.example/v1".into()),
                model: Some("k3".into()),
            },
        },
    );
    cfg.agent_configurations.insert(
        "claude".into(),
        configuration_with_endpoint(
            Agent::Claude,
            "shared",
            Some("k3"),
            "https://claude.example/v1",
        ),
    );
    cfg.agent_configurations.insert(
        "kimi".into(),
        configuration_with_endpoint(Agent::Kimi, "shared", None, "https://kimi.example/v1"),
    );

    let instances = launch(&cfg, &["claude", "kimi"]);
    let env = resolve_instance_env_with(&cfg, &instances, None, "role", &NoSecrets, |_| {
        Err(std::env::VarError::NotPresent)
    })
    .unwrap();

    let claude = env.instance("claude").unwrap();
    assert_eq!(claude.agent, "claude");
    assert_eq!(claude.account_id, "shared");
    assert_eq!(
        claude.env,
        BTreeMap::from([
            ("ANTHROPIC_AUTH_TOKEN".into(), "shared-secret".into()),
            (
                "ANTHROPIC_BASE_URL".into(),
                "https://claude.example/v1".into(),
            ),
            ("ANTHROPIC_MODEL".into(), "k3".into()),
            ("ANTHROPIC_DEFAULT_OPUS_MODEL".into(), "k3".into()),
            ("ANTHROPIC_DEFAULT_SONNET_MODEL".into(), "k3".into()),
            ("ANTHROPIC_DEFAULT_HAIKU_MODEL".into(), "k3".into()),
        ])
    );

    let kimi = env.instance("kimi").unwrap();
    assert_eq!(kimi.agent, "kimi");
    assert_eq!(kimi.account_id, "shared");
    assert_eq!(
        kimi.env,
        BTreeMap::from([
            ("KIMI_API_KEY".into(), "shared-secret".into()),
            ("KIMI_BASE_URL".into(), "https://kimi.example/v1".into()),
        ])
    );
    assert!(
        !claude
            .env
            .values()
            .any(|value| value == "https://kimi.example/v1")
    );
    assert!(
        !kimi
            .env
            .values()
            .any(|value| value == "https://claude.example/v1")
    );
    assert!(
        !claude
            .env
            .values()
            .any(|value| value == "https://account.example/v1")
    );
    assert!(
        !kimi
            .env
            .values()
            .any(|value| value == "https://account.example/v1")
    );
}

#[test]
fn same_account_claude_oauth_instances_stage_distinct_endpoints_and_isolate_secrets() {
    let oauth_account = |name: &str, token: &str| AccountConfig {
        enabled: true,
        name: name.into(),
        provider: AiProvider::Anthropic,
        credential: AccountCredential::OAuthToken {
            agent: Agent::Claude,
            value: EnvValue::from(token),
        },
    };
    let mut cfg = AppConfig::default();
    cfg.accounts.insert(
        "shared".into(),
        oauth_account("Shared Claude", "shared-oauth"),
    );
    cfg.accounts.insert(
        "unselected".into(),
        oauth_account("Unselected Claude", "unselected-oauth"),
    );
    for (id, endpoint) in [
        ("claude-work", "https://work.example/v1"),
        ("claude-personal", "https://personal.example/v1"),
    ] {
        cfg.agent_configurations.insert(
            id.into(),
            configuration_with_endpoint(Agent::Claude, "shared", None, endpoint),
        );
    }

    let instances = launch(&cfg, &["claude-work", "claude-personal"]);
    let credentials = resolve_instance_env_with(&cfg, &instances, None, "role", &NoSecrets, |_| {
        Err(std::env::VarError::NotPresent)
    })
    .unwrap();

    assert_eq!(credentials.schema_version(), 2);
    assert_eq!(credentials.iter().count(), 2);
    for (id, endpoint) in [
        ("claude-work", "https://work.example/v1"),
        ("claude-personal", "https://personal.example/v1"),
    ] {
        let envelope = credentials.instance(id).unwrap();
        assert_eq!(envelope.agent, "claude");
        assert_eq!(envelope.account_id, "shared");
        assert_eq!(
            envelope.env,
            BTreeMap::from([
                ("ANTHROPIC_BASE_URL".into(), endpoint.into()),
                ("CLAUDE_CODE_OAUTH_TOKEN".into(), "shared-oauth".into()),
            ])
        );
        assert!(
            !envelope
                .env
                .values()
                .any(|value| value == "unselected-oauth")
        );
    }
    assert!(!format!("{credentials:?}").contains("shared-oauth"));
    assert!(!format!("{credentials:?}").contains("unselected-oauth"));
}

#[test]
fn invalid_instance_endpoint_fails_before_secret_resolution() {
    let mut cfg = AppConfig::default();
    cfg.accounts.insert(
        "shared".into(),
        AccountConfig {
            enabled: true,
            name: "Shared Anthropic".into(),
            provider: AiProvider::Anthropic,
            credential: AccountCredential::ApiKey {
                value: EnvValue::from("$SHARED_TOKEN"),
                base_url: Some("https://account.example/v1".into()),
                model: None,
            },
        },
    );
    let instances = vec![jackin_config::ResolvedInstance {
        config_id: "claude".into(),
        agent: Agent::Claude,
        account_id: "shared".into(),
        model: None,
        base_url: Some("ftp://invalid.example/v1".into()),
        xdg_roots: None,
        label: "Claude".into(),
        synthesized: false,
    }];

    let error = resolve_instance_env_with(&cfg, &instances, None, "role", &NoSecrets, |_| {
        panic!("invalid endpoint must not resolve a fallback secret")
    })
    .unwrap_err();
    assert!(error.to_string().contains("HTTP(S) endpoint"), "{error:?}");
}

#[test]
fn synthesized_config_id_used_verbatim() {
    let mut cfg = AppConfig::default();
    cfg.accounts.insert(
        "work".into(),
        AccountConfig {
            enabled: true,
            name: "Work".into(),
            provider: AiProvider::Amp,
            credential: AccountCredential::ApiKey {
                value: EnvValue::from("test-key"),
                base_url: None,
                model: None,
            },
        },
    );
    let instances = jackin_config::resolve_launch(&cfg, None, "role", None, None).unwrap();
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].config_id, "work@amp");
    let env = resolve_instance_env_with(&cfg, &instances, None, "role", &NoSecrets, |_| {
        Err(std::env::VarError::NotPresent)
    })
    .unwrap();
    assert_eq!(
        env.for_instance("work@amp").unwrap()["AMP_API_KEY"],
        "test-key"
    );
}

#[test]
fn empty_credential_rejected() {
    let mut cfg = AppConfig::default();
    cfg.accounts.insert("work".into(), account("   "));
    cfg.agent_configurations
        .insert("primary".into(), configuration(Agent::Codex, "work"));
    let instances = launch(&cfg, &["primary"]);
    resolve_instance_env_with(&cfg, &instances, None, "role", &NoSecrets, |_| {
        Err(std::env::VarError::NotPresent)
    })
    .unwrap_err();
}

#[test]
fn on_demand_credential_rejected() {
    let mut cfg = AppConfig::default();
    cfg.accounts.insert(
        "work".into(),
        account_with_model(
            EnvValue::Extended(Extended {
                value: "test-key".into(),
                on_demand: true,
            }),
            None,
        ),
    );
    cfg.agent_configurations
        .insert("primary".into(), configuration(Agent::Codex, "work"));
    let instances = launch(&cfg, &["primary"]);
    let error = resolve_instance_env_with(&cfg, &instances, None, "role", &NoSecrets, |_| {
        Err(std::env::VarError::NotPresent)
    })
    .unwrap_err();
    assert!(
        error.to_string().contains("must resolve at launch"),
        "{error:?}"
    );
}

#[test]
fn generic_env_cannot_bypass_account_admission() {
    let mut cfg = AppConfig::default();
    cfg.env
        .insert("OPENAI_API_KEY".into(), EnvValue::from("test-bypass"));
    cfg.env
        .insert("MOONSHOT_API_KEY".into(), EnvValue::from("test-bypass"));
    for key in [
        "KIMI_AUTH_TOKEN",
        "kimi_auth_token",
        "MINIMAX_CODING_API_KEY",
        "Z_AI_API_KEY",
        "GOOGLE_API_KEY",
    ] {
        cfg.env.insert(key.into(), EnvValue::from("test-bypass"));
    }
    cfg.env.insert("EDITOR".into(), EnvValue::from("vim"));
    let env = crate::resolve_operator_env_with(&cfg, None, None, &NoSecrets, |_| {
        Err(std::env::VarError::NotPresent)
    })
    .unwrap();
    assert_eq!(env, BTreeMap::from([("EDITOR".into(), "vim".into())]));
}
