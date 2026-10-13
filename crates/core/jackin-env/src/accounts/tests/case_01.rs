// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
fn duplicate_instance_identity_fails_before_resolving_any_credentials() {
    let claude_account = |name: &str, value: &str| AccountConfig {
        enabled: true,
        name: name.into(),
        provider: AiProvider::Anthropic,
        credential: AccountCredential::ApiKey {
            value: EnvValue::from(value),
            base_url: None,
            model: None,
        },
    };
    let mut cfg = AppConfig::default();
    cfg.accounts.insert(
        "work".into(),
        claude_account("Work Claude", "$WORK_ACCOUNT_TOKEN"),
    );
    cfg.accounts.insert(
        "personal".into(),
        claude_account("Personal Claude", "$PERSONAL_ACCOUNT_TOKEN"),
    );
    let instances = [
        jackin_config::ResolvedInstance {
            config_id: "claude-main".into(),
            agent: Agent::Claude,
            account_id: "work".into(),
            model: None,
            base_url: None,
            xdg_roots: None,
            label: "Work Claude".into(),
            synthesized: false,
        },
        jackin_config::ResolvedInstance {
            config_id: "claude-main".into(),
            agent: Agent::Claude,
            account_id: "personal".into(),
            model: None,
            base_url: None,
            xdg_roots: None,
            label: "Personal Claude".into(),
            synthesized: false,
        },
    ];

    let error = resolve_instance_env_with(&cfg, &instances, None, "role", &NoSecrets, |_| {
        panic!("duplicate instance identity must fail before secret resolution")
    })
    .unwrap_err();

    assert!(
        error.to_string().contains("duplicate account instance id"),
        "{error:#}"
    );
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
